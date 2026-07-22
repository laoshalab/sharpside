#!/usr/bin/env python3
"""Generate Sharpside i18n message catalogs for 10 locales."""
from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

OUT_DIR = Path(__file__).resolve().parent / "messages"
LOCALES = ["zh", "en", "ja", "ko", "es", "fr", "de", "pt", "ru", "ar"]


def L(zh: str, en: str, ja: str, ko: str, es: str, fr: str, de: str, pt: str, ru: str, ar: str) -> dict[str, str]:
    return dict(zip(LOCALES, [zh, en, ja, ko, es, fr, de, pt, ru, ar]))


def deep_merge(a: dict, b: dict) -> dict:
    out = dict(a)
    for k, v in b.items():
        if k in out and isinstance(out[k], dict) and isinstance(v, dict):
            out[k] = deep_merge(out[k], v)
        else:
            out[k] = v
    return out


def flatten(d: dict, prefix: str = "") -> dict[str, str]:
    out: dict[str, str] = {}
    for k, v in d.items():
        key = f"{prefix}.{k}" if prefix else k
        if isinstance(v, dict):
            if all(isinstance(x, str) for x in v.values()):
                # locale leaf dict from L()
                if set(v.keys()) == set(LOCALES):
                    for loc in LOCALES:
                        out[f"__{loc}__{key}"] = v[loc]
                else:
                    out.update(flatten(v, key))
            else:
                out.update(flatten(v, key))
        else:
            out[key] = v
    return out


def pick_locale(node: Any, loc: str) -> Any:
    if isinstance(node, dict) and set(node.keys()) == set(LOCALES):
        return node[loc]
    if isinstance(node, dict):
        return {k: pick_locale(v, loc) for k, v in node.items()}
    return node


def to_js_module(obj: dict) -> str:
    body = json.dumps(obj, ensure_ascii=False, indent=2)
    return f"export default {body};\n"


def validate_parity(catalogs: dict[str, dict]) -> tuple[bool, str]:
    base = set(flatten(catalogs["zh"]).keys())
    # flatten locale-aware tree using zh pick
    def flat_loc(cat: dict, loc: str) -> set[str]:
        picked = pick_locale(cat, loc)
        keys: set[str] = set()

        def walk(d: dict, prefix: str = "") -> None:
            for k, v in d.items():
                key = f"{prefix}.{k}" if prefix else k
                if isinstance(v, dict):
                    walk(v, key)
                else:
                    keys.add(key)

        walk(picked)
        return keys

    base_keys = flat_loc(CATALOG, "zh")
    issues = []
    for loc in LOCALES:
        keys = flat_loc(CATALOG, loc)
        missing = base_keys - keys
        extra = keys - base_keys
        if missing:
            issues.append(f"{loc} missing: {sorted(missing)[:5]}...")
        if extra:
            issues.append(f"{loc} extra: {sorted(extra)[:5]}...")
    if issues:
        return False, "; ".join(issues)
    return True, "OK"


# ── Message catalog (leaf values are L(...) dicts) ──────────────────────────

CATALOG: dict = {
    "meta": {
        "title": L(
            "Sharpside · 多平台预测市场跟单",
            "Sharpside · Multi-venue prediction market copy trading",
            "Sharpside · マルチ Venue 予測市場コピー取引",
            "Sharpside · 멀티 Venue 예측 시장 카피 트레이딩",
            "Sharpside · Copy trading en mercados de predicción multi-Venue",
            "Sharpside · Copy trading sur marchés prédictifs multi-Venue",
            "Sharpside · Copy-Trading an Multi-Venue-Prognosemärkten",
            "Sharpside · Copy trading em mercados de previsão multi-Venue",
            "Sharpside · Копитрейдинг на прогнозных рынках (multi-Venue)",
            "Sharpside · نسخ التداول في أسواق التوقعات متعددة المنصات",
        ),
    },
    "common": {
        "notFound": L("页面不存在", "Page not found", "ページが見つかりません", "페이지를 찾을 수 없습니다", "Página no encontrada", "Page introuvable", "Seite nicht gefunden", "Página não encontrada", "Страница не найдена", "الصفحة غير موجودة"),
        "backHome": L("返回首页", "Back to home", "ホームに戻る", "홈으로", "Volver al inicio", "Retour à l'accueil", "Zur Startseite", "Voltar ao início", "На главную", "العودة للرئيسية"),
        "loadFailed": L("加载失败", "Failed to load", "読み込みに失敗しました", "로드 실패", "Error al cargar", "Échec du chargement", "Laden fehlgeschlagen", "Falha ao carregar", "Ошибка загрузки", "فشل التحميل"),
        "loadFailedColon": L("加载失败：{msg}", "Failed to load: {msg}", "読み込み失敗：{msg}", "로드 실패: {msg}", "Error al cargar: {msg}", "Échec du chargement : {msg}", "Laden fehlgeschlagen: {msg}", "Falha ao carregar: {msg}", "Ошибка загрузки: {msg}", "فشل التحميل: {msg}"),
        "connectFailed": L("连接失败", "Connection failed", "接続に失敗しました", "연결 실패", "Conexión fallida", "Échec de la connexion", "Verbindung fehlgeschlagen", "Falha na conexão", "Ошибка подключения", "فشل الاتصال"),
        "loginRequired": L("需要先连接钱包", "Connect your wallet to continue", "続行するにはウォレットを接続してください", "계속하려면 지갑을 연결하세요", "Conecta tu wallet para continuar", "Connectez votre wallet pour continuer", "Wallet verbinden, um fortzufahren", "Conecte a wallet para continuar", "Подключите кошелёк для продолжения", "يجب ربط المحفظة للمتابعة"),
        "loginRequiredHint": L("连接后即可使用跟单、组合与设置。", "After connecting you can use Copy, Portfolio, and Settings.", "接続後、コピー・ポートフォリオ・設定が利用できます。", "연결 후 Copy, Portfolio, 설정을 사용할 수 있습니다.", "Tras conectar puedes usar Copy, Portfolio y Ajustes.", "Après connexion : Copy, Portfolio et Paramètres.", "Nach Verbindung: Copy, Portfolio und Einstellungen.", "Após conectar: Copy, Portfolio e Configurações.", "После подключения: Copy, Portfolio и Настройки.", "بعد الربط يمكنك استخدام النسخ والمحفظة والإعدادات."),
        "cancel": L("取消", "Cancel", "キャンセル", "취소", "Cancelar", "Annuler", "Abbrechen", "Cancelar", "Отмена", "إلغاء"),
        "copy": L("复制", "Copy", "コピー", "복사", "Copiar", "Copier", "Kopieren", "Copiar", "Копировать", "نسخ"),
        "copied": L("已复制", "Copied", "コピーしました", "복사됨", "Copiado", "Copié", "Kopiert", "Copiado", "Скопировано", "تم النسخ"),
        "retry": L("重试", "Retry", "再試行", "재시도", "Reintentar", "Réessayer", "Erneut versuchen", "Tentar novamente", "Повторить", "إعادة المحاولة"),
        "manage": L("管理", "Manage", "管理", "관리", "Gestionar", "Gérer", "Verwalten", "Gerir", "Управление", "إدارة"),
        "exportCsv": L("导出 CSV", "Export CSV", "CSV をエクスポート", "CSV 내보내기", "Exportar CSV", "Exporter CSV", "CSV exportieren", "Exportar CSV", "Экспорт CSV", "تصدير CSV"),
        "all": L("全部", "All", "すべて", "전체", "Todo", "Tout", "Alle", "Todos", "Все", "الكل"),
        "loading": L("加载中…", "Loading…", "読み込み中…", "로딩 중…", "Cargando…", "Chargement…", "Laden…", "A carregar…", "Загрузка…", "جاري التحميل…"),
        "confirm": L("确认", "Confirm", "確認", "확인", "Confirmar", "Confirmer", "Bestätigen", "Confirmar", "Подтвердить", "تأكيد"),
        "delete": L("删除", "Delete", "削除", "삭제", "Eliminar", "Supprimer", "Löschen", "Eliminar", "Удалить", "حذف"),
        "save": L("保存", "Save", "保存", "저장", "Guardar", "Enregistrer", "Speichern", "Guardar", "Сохранить", "حفظ"),
        "close": L("关闭", "Close", "閉じる", "닫기", "Cerrar", "Fermer", "Schließen", "Fechar", "Закрыть", "إغلاق"),
        "period": {
            "1d": L("1天", "1D", "1日", "1일", "1D", "1J", "1T", "1D", "1Д", "يوم"),
            "1w": L("1周", "1W", "1週", "1주", "1S", "1S", "1W", "1S", "1Н", "أسبوع"),
            "1m": L("1个月", "1M", "1か月", "1개월", "1M", "1M", "1M", "1M", "1М", "شهر"),
            "1y": L("1年", "1Y", "1年", "1년", "1A", "1A", "1J", "1A", "1Г", "سنة"),
            "ytd": L("年初至今", "YTD", "年初来", "YTD", "YTD", "AAJ", "YTD", "YTD", "С нач. года", "منذ بداية العام"),
            "all": L("全部", "All", "すべて", "전체", "Todo", "Tout", "Alle", "Todos", "Все", "الكل"),
        },
        "unlimited": L("不限", "Unlimited", "無制限", "무제한", "Sin límite", "Illimité", "Unbegrenzt", "Ilimitado", "Без лимита", "غير محدود"),
        "unknown": L("未知", "Unknown", "不明", "알 수 없음", "Desconocido", "Inconnu", "Unbekannt", "Desconhecido", "Неизвестно", "غير معروف"),
        "submitting": L("提交中…", "Submitting…", "送信中…", "제출 중…", "Enviando…", "Envoi…", "Wird gesendet…", "A enviar…", "Отправка…", "جاري الإرسال…"),
    },
}

# Import extended namespaces from companion module if present; else inline below
try:
    from _gen_sharpside_i18n_data import EXTEND  # noqa: F401
    CATALOG = deep_merge(CATALOG, EXTEND)
except ImportError:
    pass


def build_all() -> dict[str, dict]:
    return {loc: pick_locale(CATALOG, loc) for loc in LOCALES}


def main() -> int:
    ok, msg = validate_parity({"zh": pick_locale(CATALOG, "zh")})
    # fix validate to use CATALOG directly
    base_keys: set[str] = set()

    def walk(d: dict, prefix: str = "") -> None:
        for k, v in d.items():
            key = f"{prefix}.{k}" if prefix else k
            if isinstance(v, dict) and set(v.keys()) == set(LOCALES):
                base_keys.add(key)
            elif isinstance(v, dict):
                walk(v, key)

    walk(CATALOG)
    catalogs = build_all()
    for loc in LOCALES:
        picked = catalogs[loc]

        def walk2(d: dict, prefix: str = "") -> set[str]:
            keys: set[str] = set()
            for k, v in d.items():
                key = f"{prefix}.{k}" if prefix else k
                if isinstance(v, dict):
                    keys |= walk2(v, key)
                else:
                    keys.add(key)
            return keys

        loc_keys = walk2(picked)
        if loc_keys != base_keys:
            print(f"PARITY FAIL {loc}: missing={base_keys - loc_keys} extra={loc_keys - base_keys}", file=sys.stderr)
            return 1

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    for loc, cat in catalogs.items():
        (OUT_DIR / f"{loc}.js").write_text(to_js_module(cat), encoding="utf-8")

    print(f"zh key count: {len(base_keys)}")
    print("parity: OK")
    print("namespaces:", ", ".join(sorted(CATALOG.keys())))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
