#!/usr/bin/env python3
"""本地 Mock Polymarket API —— 真实 DTO 形状的 fixture 数据。

用于在无法直连 api.polymarket.com 的环境（如离线/受限网络）联调 venue-hub
全链路：leaderboard → traders、markets → raw_markets、trades → raw_trades、
positions、book。对应 `crates/venues/polymarket/src/dto.rs` 的字段名（camelCase）。

用法：python3 infra/mock/polymarket_mock.py [port]
默认端口 9200。venue-hub 设：
  POLYMARKET_DATA_API_URL=http://127.0.0.1:9200
  POLYMARKET_GAMMA_API_URL=http://127.0.0.1:9200
  POLYMARKET_CLOB_API_URL=http://127.0.0.1:9200
（三个 API 合一，按 path 路由。）
"""
import json
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse, parse_qs

# ── fixture：交易者（leaderboard）── 真实形状的 proxyWallet 地址
TRADERS = [
    {"rank": "1", "proxyWallet": "0x1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b",
     "userName": "AlphaWhale", "vol": 1234567.8, "pnl": 98765.4,
     "profileImage": "https://img.example/alpha.png", "xUsername": "alphawhale_x",
     "verifiedBadge": True},
    {"rank": "2", "proxyWallet": "0x2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c",
     "userName": "DiamondHands", "vol": 987654.3, "pnl": 54321.0,
     "profileImage": None, "xUsername": "diamondx", "verifiedBadge": False},
    {"rank": "3", "proxyWallet": "0x3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d",
     "userName": "PolyPro", "vol": 765432.1, "pnl": 32100.5,
     "profileImage": "https://img.example/poly.png", "xUsername": None,
     "verifiedBadge": True},
    {"rank": "4", "proxyWallet": "0x4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e",
     "userName": "NoCoinBet", "vol": 543210.0, "pnl": -1234.5,
     "profileImage": None, "xUsername": "nocoin", "verifiedBadge": False},
    {"rank": "5", "proxyWallet": "0x5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f",
     "userName": "SharpSide", "vol": 432100.0, "pnl": 21000.0,
     "profileImage": "https://img.example/sharp.png", "xUsername": "sharpside",
     "verifiedBadge": True},
]

# ── fixture：市场（Gamma /markets）── 64-hex conditionId
MARKETS = [
    {"id": "m1", "conditionId": "0x6d081aa1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9",
     "question": "Will BTC close above $100k on Dec 31?", "slug": "btc-100k-dec31",
     "tags": ["crypto", "bitcoin"], "endDate": "2026-12-31T23:59:00Z",
     "outcomes": ["Yes", "No"]},
    {"id": "m2", "conditionId": "0x7e192bb2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0",
     "question": "Will the Fed cut rates in July?", "slug": "fed-cut-july",
     "tags": ["economics", "fed"], "endDate": "2026-07-31T23:59:00Z",
     "outcomes": ["Yes", "No"]},
    {"id": "m3", "conditionId": "0x8f2a3cc3d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1",
     "question": "Will GDP growth exceed 2% in Q2?", "slug": "gdp-q2-2pct",
     "tags": ["economics"], "endDate": "2026-09-30T23:59:00Z",
     "outcomes": ["Yes", "No"]},
]

# token asset id（每个市场 YES token）
T_YES = "6801"
T_NO = "6802"
COND = MARKETS[0]["conditionId"]

# ── fixture：成交（Data API /trades?user=）──
# 对任意 user 返回一组真实形状的成交：一个已平仓（BUY 100@0.40 → SELL 100@0.60，realized +20）
# + 一个未平仓（BUY 200@0.30）。时间戳分布在最近 10 天，确保 1d/1w/1m/1y/ytd/all 都有数据。
_NOW = int(time.time())
_TS = [_NOW - 8*86400, _NOW - 6*86400, _NOW - 4*86400, _NOW - 2*86400, _NOW - 1*86400]


def trades_for(user: str):
    return [
        {"id": f"{user}-t1", "side": "BUY", "size": 100.0, "price": 0.40,
         "timestamp": str(_TS[0]), "market": COND, "asset": T_YES, "tradeOwner": user},
        {"id": f"{user}-t2", "side": "SELL", "size": 100.0, "price": 0.60,
         "timestamp": str(_TS[1]), "market": COND, "asset": T_YES, "tradeOwner": user},
        {"id": f"{user}-t3", "side": "BUY", "size": 200.0, "price": 0.30,
         "timestamp": str(_TS[2]), "market": COND, "asset": T_YES, "tradeOwner": user},
        {"id": f"{user}-t4", "side": "BUY", "size": 50.0, "price": 0.35,
         "timestamp": str(_TS[3]), "market": COND, "asset": T_YES, "tradeOwner": user},
        {"id": f"{user}-t5", "side": "SELL", "size": 50.0, "price": 0.55,
         "timestamp": str(_TS[4]), "market": COND, "asset": T_YES, "tradeOwner": user},
    ]


def positions_for(user: str):
    return [
        {"user": user, "market": COND, "asset": T_YES, "size": 200.0,
         "avgPrice": 0.30, "currentPrice": 0.45, "realizedPnl": 30.0, "side": "YES"},
    ]


def book_for(market: str, asset: str):
    return {"bids": [{"price": "0.44", "size": "150"}, {"price": "0.43", "size": "300"}],
            "asks": [{"price": "0.46", "size": "120"}, {"price": "0.47", "size": "200"}]}


class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        pass

    def _send(self, obj):
        body = json.dumps(obj).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        p = urlparse(self.path)
        q = parse_qs(p.query)
        # 兼容真实 API 的 `/v1/` 前缀（client 打 /v1/leaderboard，positions/trades 走根路径）。
        path = p.path.rstrip("/") or "/"
        if path.startswith("/v1/"):
            path = path[len("/v1"):]
        if path == "/leaderboard":
            self._send(TRADERS)
        elif path == "/markets":
            self._send(MARKETS)
        elif path == "/trades":
            user = q.get("user", [""])[0]
            self._send(trades_for(user) if user else [])
        elif path == "/positions":
            user = q.get("user", [""])[0]
            self._send(positions_for(user) if user else [])
        elif path == "/value":
            # 当前持仓总估值快照（非时间序列）。fixture：按 trader 名派生稳定假值。
            user = q.get("user", [""])[0]
            base = sum(ord(c) for c in user) % 10000
            self._send([{"user": user, "value": float(100000 + base)}])
        elif path == "/book":
            self._send(book_for(q.get("market", [""])[0], q.get("asset", [""])[0]))
        else:
            self.send_error(404, f"unknown path {path}")


def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 9200
    srv = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    print(f"mock polymarket listening on 127.0.0.1:{port}")
    srv.serve_forever()


if __name__ == "__main__":
    main()
