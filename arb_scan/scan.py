#!/usr/bin/env python3
"""
Polymarket 套利机会扫描器

扫描策略：
1. 单市场 YES+NO 价差套利（YES + NO < 1.00 时买入两边）
2. 多选项互斥市场套利（所有选项 YES 之和 < 1.00 时买入全部）
3. 同事件多市场逻辑不一致（粗略）

数据源：Polymarket Gamma API（公开市场列表）+ CLOB API（订单簿报价）
"""

import requests
import time
from collections import defaultdict

GAMMA_URL = "https://gamma-api.polymarket.com/markets"
CLOB_URL = "https://clob.polymarket.com/markets"

SESSION = requests.Session()
SESSION.headers.update({"User-Agent": "arb-scanner/1.0"})
SESSION.proxies.update({
    "http": "http://127.0.0.1:7890",
    "https": "http://127.0.0.1:7890",
})


def fetch_markets(limit=500, offset=0):
    """从 Gamma API 拉取活跃市场列表"""
    params = {
        "limit": limit,
        "offset": offset,
        "active": "true",
        "closed": "false",
        "order": "volume24hr",
        "ascending": "false",
    }
    r = SESSION.get(GAMMA_URL, params=params, timeout=30)
    r.raise_for_status()
    return r.json()


def fetch_book(token_id):
    """拉取某个 outcome token 的订单簿"""
    url = f"https://clob.polymarket.com/book?token_id={token_id}"
    try:
        r = SESSION.get(url, timeout=15)
        r.raise_for_status()
        return r.json()
    except Exception as e:
        return None


def best_prices(book):
    """从订单簿提取最优买一/卖一"""
    try:
        bids = book.get("bids") or []
        asks = book.get("asks") or []
        best_bid = float(bids[0]["price"]) if bids else 0.0
        best_ask = float(asks[0]["price"]) if asks else 1.0
        return best_bid, best_ask
    except Exception:
        return 0.0, 1.0


def scan_yes_no_arb(market):
    """单市场 YES+NO 套利：用卖一价（你买入价）"""
    outcomes = market.get("outcomes") or []
    clob_token_ids = market.get("clobTokenIds") or []
    if len(outcomes) != 2 or len(clob_token_ids) != 2:
        return None

    # 解析 token ids（可能是字符串 JSON 数组）
    if isinstance(clob_token_ids, str):
        try:
            import json
            clob_token_ids = json.loads(clob_token_ids)
        except Exception:
            return None

    prices = []
    for tid in clob_token_ids:
        book = fetch_book(tid)
        if not book:
            return None
        _, best_ask = best_prices(book)
        prices.append(best_ask)

    yes_ask, no_ask = prices[0], prices[1]
    total = yes_ask + no_ask
    # 套利空间：买入两边，结算拿回 1.00
    profit = 1.00 - total
    if profit > 0.005:  # > 0.5% 才值得报
        return {
            "type": "YES/NO 价差",
            "question": market.get("question"),
            "yes_ask": yes_ask,
            "no_ask": no_ask,
            "total_cost": total,
            "profit_per_share": profit,
            "roi_pct": round(profit / total * 100, 2),
            "url": f"https://polymarket.com/event/{market.get('slug', '')}",
        }
    return None


def scan_multi_option_arb(markets_by_group):
    """多选项互斥市场：所有 YES 之和 < 1.00"""
    results = []
    for group_key, markets in markets_by_group.items():
        if len(markets) < 3:
            continue
        total_ask = 0.0
        details = []
        for m in markets:
            clob_token_ids = m.get("clobTokenIds") or []
            if isinstance(clob_token_ids, str):
                try:
                    import json
                    clob_token_ids = json.loads(clob_token_ids)
                except Exception:
                    clob_token_ids = []
            if not clob_token_ids:
                continue
            book = fetch_book(clob_token_ids[0])
            if not book:
                continue
            _, best_ask = best_prices(book)
            total_ask += best_ask
            details.append((m.get("question", "")[:50], best_ask))

        if len(details) >= 3 and total_ask < 0.99:
            profit = 1.00 - total_ask
            results.append({
                "type": "多选项互斥",
                "group": group_key,
                "n_options": len(details),
                "total_cost": round(total_ask, 4),
                "profit_per_share": round(profit, 4),
                "roi_pct": round(profit / total_ask * 100, 2) if total_ask > 0 else 0,
                "options": details,
            })
    return results


def main():
    print("=" * 70)
    print("Polymarket 套利机会扫描")
    print("=" * 70)
    print(f"扫描时间: {time.strftime('%Y-%m-%d %H:%M:%S')}\n")

    # 拉取前 500 个活跃市场（按 24h 成交量排序）
    print("[1/3] 拉取活跃市场列表...")
    all_markets = []
    for offset in (0, 500):
        try:
            batch = fetch_markets(limit=500, offset=offset)
            if not batch:
                break
            all_markets.extend(batch)
            print(f"  已拉取 {len(all_markets)} 个市场")
        except Exception as e:
            print(f"  拉取失败 offset={offset}: {e}")
            break

    if not all_markets:
        print("未拉到任何市场，可能 API 限流或网络问题。")
        return

    # 过滤二元市场（YES/NO）
    binary_markets = [m for m in all_markets if m.get("outcomes")
                      and (isinstance(m["outcomes"], str) or isinstance(m["outcomes"], list))]
    print(f"  其中二元市场候选: {len(binary_markets)} 个\n")

    # === 扫描 YES/NO 套利 ===
    print("[2/3] 扫描 YES+NO 价差套利（取卖一价，即你买入成本）...")
    yesno_opps = []
    for i, m in enumerate(binary_markets):
        if i % 50 == 0:
            print(f"  进度 {i}/{len(binary_markets)}")
        opp = scan_yes_no_arb(m)
        if opp:
            yesno_opps.append(opp)
        time.sleep(0.05)  # 礼貌限速

    print(f"\n  发现 YES/NO 套利机会: {len(yesno_opps)} 个\n")
    if yesno_opps:
        yesno_opps.sort(key=lambda x: -x["roi_pct"])
        for opp in yesno_opps[:20]:
            print(f"  [{opp['roi_pct']}% ROI] {opp['question']}")
            print(f"    YES买价={opp['yes_ask']:.4f}  NO买价={opp['no_ask']:.4f}  "
                  f"合计={opp['total_cost']:.4f}  每份利润={opp['profit_per_share']:.4f}")
            print(f"    {opp['url']}\n")

    # 即使没套利，也展示"最紧"的几个市场（合计最低）
    print("  --- YES+NO 合计最低的 10 个市场（看市场有多紧） ---")
    tightest = []
    for m in binary_markets:
        clob_token_ids = m.get("clobTokenIds") or []
        if isinstance(clob_token_ids, str):
            try:
                import json
                clob_token_ids = json.loads(clob_token_ids)
            except Exception:
                continue
        if len(clob_token_ids) != 2:
            continue
        b1 = fetch_book(clob_token_ids[0])
        b2 = fetch_book(clob_token_ids[1])
        if not b1 or not b2:
            continue
        _, ask1 = best_prices(b1)
        _, ask2 = best_prices(b2)
        tightest.append({
            "q": (m.get("question") or "")[:70],
            "sum": ask1 + ask2,
            "yes": ask1,
            "no": ask2,
            "bid1": best_prices(b1)[0],
            "bid2": best_prices(b2)[0],
        })
    tightest.sort(key=lambda x: x["sum"])
    for t in tightest[:10]:
        edge = 1.0 - t["sum"]
        sign = "+" if edge > 0 else ""
        print(f"  合计={t['sum']:.4f}  边际={sign}{edge:.4f}  "
              f"YES={t['yes']:.4f}/{t['bid1']:.4f}  NO={t['no']:.4f}/{t['bid2']:.4f}  "
              f"| {t['q']}")
    print()

    # === 扫描多选项互斥套利 ===
    print("[3/3] 扫描多选项互斥市场套利（按 eventSlug 分组）...")
    by_event = defaultdict(list)
    for m in all_markets:
        slug = m.get("eventSlug") or m.get("slug")
        if slug:
            by_event[slug].append(m)

    multi_opps = scan_multi_option_arb(by_event)
    print(f"\n  发现多选项互斥套利机会: {len(multi_opps)} 个\n")
    if multi_opps:
        multi_opps.sort(key=lambda x: -x["roi_pct"])
        for opp in multi_opps[:10]:
            print(f"  [{opp['roi_pct']}% ROI] 事件: {opp['group']}  "
                  f"({opp['n_options']} 选项)")
            print(f"    合计成本={opp['total_cost']:.4f}  "
                  f"每份利润={opp['profit_per_share']:.4f}")
            for q, p in opp["options"][:5]:
                print(f"      - {q}: {p:.4f}")
            print()

    # === 汇总 ===
    print("=" * 70)
    print("汇总")
    print("=" * 70)
    print(f"扫描市场总数: {len(all_markets)}")
    print(f"YES/NO 套利机会 (>0.5%): {len(yesno_opps)}")
    print(f"多选项互斥套利机会: {len(multi_opps)}")
    if not yesno_opps and not multi_opps:
        print("\n当前未发现明显套利空间——这与我们之前讨论的结论一致：")
        print("机器人已基本吃光简单套利机会。建议转向事件驱动交易或做市。")


if __name__ == "__main__":
    main()
