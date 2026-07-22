#!/usr/bin/env python3
"""Generate de.js, ru.js, ar.js from en.js + embedded TRANSLATIONS."""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

DIR = Path(__file__).resolve().parent / "messages"
LOCALES = ("de", "ru", "ar")

TRANSLATIONS: dict[str, dict[str, str]] = {
  "meta.title": {
    "de": "Sharpside · Copy-Trading an Multi-Venue-Prognosemärkten",
    "ru": "Sharpside · Копитрейдинг на прогнозных рынках (multi-Venue)",
    "ar": "Sharpside · نسخ التداول في أسواق التوقعات متعددة Venue"
  },
  "common.notFound": {
    "de": "Seite nicht gefunden",
    "ru": "Страница не найдена",
    "ar": "الصفحة غير موجودة"
  },
  "common.backHome": {
    "de": "Zur Startseite",
    "ru": "На главную",
    "ar": "العودة للرئيسية"
  },
  "common.loadFailed": {
    "de": "Laden fehlgeschlagen",
    "ru": "Ошибка загрузки",
    "ar": "فشل التحميل"
  },
  "common.loadFailedColon": {
    "de": "Laden fehlgeschlagen: {msg}",
    "ru": "Ошибка загрузки: {msg}",
    "ar": "فشل التحميل: {msg}"
  },
  "common.connectFailed": {
    "de": "Verbindung fehlgeschlagen",
    "ru": "Ошибка подключения",
    "ar": "فشل الاتصال"
  },
  "common.loginRequired": {
    "de": "Wallet verbinden, um fortzufahren",
    "ru": "Подключите кошелёк для продолжения",
    "ar": "يجب ربط المحفظة للمتابعة"
  },
  "common.loginRequiredHint": {
    "de": "Nach der Verbindung stehen Copy, Portfolio und Einstellungen zur Verfügung.",
    "ru": "После подключения доступны Copy, Portfolio и Настройки.",
    "ar": "بعد الربط يمكنك استخدام النسخ والمحفظة والإعدادات."
  },
  "common.cancel": {
    "de": "Abbrechen",
    "ru": "Отмена",
    "ar": "إلغاء"
  },
  "common.copy": {
    "de": "Kopieren",
    "ru": "Копировать",
    "ar": "نسخ"
  },
  "common.copied": {
    "de": "Kopiert",
    "ru": "Скопировано",
    "ar": "تم النسخ"
  },
  "common.retry": {
    "de": "Erneut versuchen",
    "ru": "Повторить",
    "ar": "إعادة المحاولة"
  },
  "common.manage": {
    "de": "Verwalten →",
    "ru": "Управление →",
    "ar": "إدارة →"
  },
  "common.exportCsv": {
    "de": "CSV exportieren",
    "ru": "Экспорт CSV",
    "ar": "تصدير CSV"
  },
  "common.all": {
    "de": "Alle",
    "ru": "Все",
    "ar": "الكل"
  },
  "common.loading": {
    "de": "Laden…",
    "ru": "Загрузка…",
    "ar": "جاري التحميل…"
  },
  "common.confirm": {
    "de": "Bestätigen",
    "ru": "Подтвердить",
    "ar": "تأكيد"
  },
  "common.delete": {
    "de": "Löschen",
    "ru": "Удалить",
    "ar": "حذف"
  },
  "common.save": {
    "de": "Speichern",
    "ru": "Сохранить",
    "ar": "حفظ"
  },
  "common.close": {
    "de": "Schließen",
    "ru": "Закрыть",
    "ar": "إغلاق"
  },
  "common.submitting": {
    "de": "Wird gesendet…",
    "ru": "Отправка…",
    "ar": "جاري الإرسال…"
  },
  "common.unlimited": {
    "de": "Unbegrenzt",
    "ru": "Без лимита",
    "ar": "غير محدود"
  },
  "common.unknown": {
    "de": "Unbekannt",
    "ru": "Неизвестно",
    "ar": "غير معروف"
  },
  "common.viewMore": {
    "de": "Ansehen →",
    "ru": "Смотреть →",
    "ar": "عرض →"
  },
  "common.viewAll": {
    "de": "Alle →",
    "ru": "Все →",
    "ar": "الكل →"
  },
  "common.period.1d": {
    "de": "1T",
    "ru": "1Д",
    "ar": "يوم"
  },
  "common.period.1w": {
    "de": "1W",
    "ru": "1Н",
    "ar": "أسبوع"
  },
  "common.period.1m": {
    "de": "1M",
    "ru": "1М",
    "ar": "شهر"
  },
  "common.period.1y": {
    "de": "1J",
    "ru": "1Г",
    "ar": "سنة"
  },
  "common.period.ytd": {
    "de": "YTD",
    "ru": "С нач. года",
    "ar": "منذ بداية العام"
  },
  "common.period.all": {
    "de": "Alle",
    "ru": "Все",
    "ar": "الكل"
  },
  "nav.ariaMain": {
    "de": "Hauptnavigation",
    "ru": "Основная навигация",
    "ar": "التنقل الرئيسي"
  },
  "nav.ariaMobile": {
    "de": "Mobile Navigation",
    "ru": "Мобильная навигация",
    "ar": "التنقل على الجوال"
  },
  "nav.discover": {
    "de": "Entdecken",
    "ru": "Обзор",
    "ar": "استكشاف"
  },
  "nav.copy": {
    "de": "Copy",
    "ru": "Copy",
    "ar": "نسخ"
  },
  "nav.portfolio": {
    "de": "Portfolio",
    "ru": "Portfolio",
    "ar": "المحفظة"
  },
  "nav.account": {
    "de": "Einstellungen",
    "ru": "Настройки",
    "ar": "الإعدادات"
  },
  "nav.home": {
    "de": "Start",
    "ru": "Главная",
    "ar": "الرئيسية"
  },
  "nav.leaderboard": {
    "de": "Rangliste",
    "ru": "Рейтинг",
    "ar": "لوحة المتصدرين"
  },
  "nav.watchlist": {
    "de": "Watchlist",
    "ru": "Список наблюдения",
    "ar": "قائمة المراقبة"
  },
  "nav.follows": {
    "de": "Meine Follows",
    "ru": "Мои подписки",
    "ar": "متابعاتي"
  },
  "nav.dashboard": {
    "de": "Dashboard",
    "ru": "Дашборд",
    "ar": "لوحة التحكم"
  },
  "nav.portfolioPage": {
    "de": "Portfolio",
    "ru": "Портфель",
    "ar": "المحفظة"
  },
  "nav.wallet": {
    "de": "Wallet",
    "ru": "Кошелёк",
    "ar": "المحفظة"
  },
  "nav.settings": {
    "de": "Einstellungen",
    "ru": "Настройки",
    "ar": "الإعدادات"
  },
  "nav.connectWallet": {
    "de": "Wallet verbinden",
    "ru": "Подключить кошелёк",
    "ar": "ربط المحفظة"
  },
  "nav.connectShort": {
    "de": "Verbinden",
    "ru": "Подключить",
    "ar": "ربط"
  },
  "nav.connected": {
    "de": "Verbunden",
    "ru": "Подключено",
    "ar": "متصل"
  },
  "nav.disconnect": {
    "de": "Trennen",
    "ru": "Отключить",
    "ar": "قطع الاتصال"
  },
  "nav.language": {
    "de": "Sprache",
    "ru": "Язык",
    "ar": "اللغة"
  },
  "nav.toggleTheme": {
    "de": "Theme wechseln",
    "ru": "Сменить тему",
    "ar": "تبديل السمة"
  },
  "footer.ariaNav": {
    "de": "Fußzeilen-Navigation",
    "ru": "Навигация в подвале",
    "ar": "تنقل التذييل"
  },
  "footer.ariaContact": {
    "de": "Kontakt",
    "ru": "Контакты",
    "ar": "اتصل بنا"
  },
  "footer.contact": {
    "de": "Kontakt",
    "ru": "Контакты",
    "ar": "اتصل بنا"
  },
  "footer.tagline": {
    "de": "Copy-Trading an Multi-Venue-Prognosemärkten",
    "ru": "Копитрейдинг на прогнозных рынках (multi-Venue)",
    "ar": "نسخ التداول في أسواق التوقعات متعددة Venue"
  },
  "footer.note": {
    "de": "Kanal A: delegiertes Trading (noch nicht vollständig non-custodial); Kanal B: self-hosted daemon zero-key (Pro+).",
    "ru": "Канал A — делегированная торговля (ещё не полностью non-custodial); Канал B — self-hosted daemon zero-key (Pro+).",
    "ar": "القناة A تداول مفوض (ليس non-custodial بالكامل بعد)；القناة B daemon ذاتي zero-key (Pro+)."
  },
  "home.title": {
    "de": "Copy-Trading an Multi-Venue-Prognosemärkten",
    "ru": "Копитрейдинг на прогнозных рынках (multi-Venue)",
    "ar": "نسخ التداول في أسواق التوقعات متعددة Venue"
  },
  "home.sub": {
    "de": "Top-Trader finden, mit einem Klick folgen, Portfolio analysieren.",
    "ru": "Найдите топ-трейдеров, следуйте в один клик, анализируйте портфель.",
    "ar": "اكتشف المتداولين المتميزين، تابع بنقرة، راجع محفظتك."
  },
  "home.discover": {
    "de": "Trader entdecken",
    "ru": "Найти трейдеров",
    "ar": "اكتشف المتداولين"
  },
  "home.goDashboard": {
    "de": "Zum Dashboard",
    "ru": "На дашборд",
    "ar": "إلى لوحة التحكم"
  },
  "home.connectWallet": {
    "de": "Wallet verbinden",
    "ru": "Подключить кошелёк",
    "ar": "ربط المحفظة"
  },
  "home.venue.capSignal": {
    "de": "Signal",
    "ru": "Сигнал",
    "ar": "إشارة"
  },
  "home.venue.capExecution": {
    "de": "Execution",
    "ru": "Исполнение",
    "ar": "التنفيذ"
  },
  "home.venue.authNone": {
    "de": "Keine Auth",
    "ru": "Без аутентификации",
    "ar": "بدون مصادقة"
  },
  "home.venue.geoUsRestrict": {
    "de": "Global (US-Beschränkungen)",
    "ru": "Global (ограничения США)",
    "ar": "Global (قيود أمريكية)"
  },
  "home.venue.phaseDefault": {
    "de": "Demnächst",
    "ru": "Скоро",
    "ar": "قريباً"
  },
  "home.venue.liveBadge": {
    "de": "Live",
    "ru": "Активно",
    "ar": "متاح"
  },
  "home.venue.ariaLocked": {
    "de": "{name}, {phase}, demnächst",
    "ru": "{name}, {phase}, скоро",
    "ar": "{name}، {phase}، قريباً"
  },
  "home.venue.toastComingSoon": {
    "de": "{name} ({phase}) demnächst",
    "ru": "{name} ({phase}) скоро",
    "ar": "{name} ({phase}) قريباً"
  },
  "home.venue.lockOverlay": {
    "de": "Demnächst · Roadmap",
    "ru": "Скоро · roadmap",
    "ar": "قريباً · خارطة الطريق"
  },
  "home.venue.ariaLive": {
    "de": "{name}, live, Rangliste ansehen",
    "ru": "{name}, активен, рейтинг",
    "ar": "{name}، متاح، عرض لوحة المتصدرين"
  },
  "home.venue.sectionTitle": {
    "de": "Live & demnächst",
    "ru": "Активные и скоро",
    "ar": "متاح وقريباً"
  },
  "home.venue.sectionDesc": {
    "de": "Ein Terminal für alle Prognosemärkte.",
    "ru": "Один терминал — множество рынков.",
    "ar": "محطة واحدة لأسواق التوقعات."
  },
  "home.venue.emptyTitle": {
    "de": "Noch keine Venues verbunden",
    "ru": "Нет подключённых Venue",
    "ar": "لا Venue متصل بعد"
  },
  "home.venue.emptyHint": {
    "de": "Verbundene Märkte erscheinen hier.",
    "ru": "Подключённые рынки появятся здесь.",
    "ar": "ستظهر الأسواق المتصلة هنا."
  },
  "home.venue.loadError": {
    "de": "Venues konnten nicht geladen werden",
    "ru": "Не удалось загрузить Venue",
    "ar": "تعذّر تحميل Venue"
  },
  "home.channels.ctaSetupFollows": {
    "de": "Follows einrichten",
    "ru": "Настроить follows",
    "ar": "إعداد المتابعات"
  },
  "home.channels.ctaConnectStart": {
    "de": "Wallet verbinden zum Start",
    "ru": "Подключите кошелёк для начала",
    "ar": "اربط المحفظة للبدء"
  },
  "home.channels.ctaConnectUpgrade": {
    "de": "Wallet verbinden zum Upgrade",
    "ru": "Подключите кошелёк для upgrade",
    "ar": "اربط المحفظة للترقية"
  },
  "home.channels.ctaConfigureDaemon": {
    "de": "daemon konfigurieren",
    "ru": "Настроить daemon",
    "ar": "إعداد daemon"
  },
  "home.channels.ctaUpgradePro": {
    "de": "Auf Pro+ upgraden",
    "ru": "Upgrade Pro+",
    "ar": "ترقية Pro+"
  },
  "home.channels.sectionTitle": {
    "de": "Zwei Copy-Wege",
    "ru": "Два способа копитрейдинга",
    "ar": "طريقتان للنسخ"
  },
  "home.channels.sectionDesc": {
    "de": "Wählen Sie Ihr Kontrollniveau. Custody wird klar benannt.",
    "ru": "Выберите уровень контроля. Custody прозрачно.",
    "ar": "اختر مستوى التحكم. نحدد Custody بوضوح."
  },
  "home.channels.aTag": {
    "de": "Kanal A",
    "ru": "Канал A",
    "ar": "القناة A"
  },
  "home.channels.aTitle": {
    "de": "TG Deposit Wallet delegierte Signatur",
    "ru": "Делегированная подпись TG Deposit Wallet",
    "ar": "توقيع مفوض TG Deposit Wallet"
  },
  "home.channels.aLead": {
    "de": "Nach Login sofort starten — ideal zum Einstieg.",
    "ru": "Войдите и начните — удобно для старта.",
    "ar": "سجّل الدخول وابدأ — مناسب للبدء."
  },
  "home.channels.aPoint1": {
    "de": "Direkt nach Wallet-Login folgen",
    "ru": "Follow после входа в кошелёк",
    "ar": "متابعة بعد ربط المحفظة"
  },
  "home.channels.aPoint2": {
    "de": "Copy-Trades via Telegram / Web",
    "ru": "Copy через Telegram / web",
    "ar": "نسخ عبر Telegram / الويب"
  },
  "home.channels.aPoint3": {
    "de": "Plattform signiert für Sie (keine eigenen Keys)",
    "ru": "Платформа подписывает (без своих ключей)",
    "ar": "المنصة توقّع نيابةً عنك (بدون مفاتيح ذاتية)"
  },
  "home.channels.aCustody": {
    "de": "⚠ Delegiertes Trading · noch nicht vollständig non-custodial",
    "ru": "⚠ Делегированная торговля · ещё не полностью non-custodial",
    "ar": "⚠ تداول مفوض · ليس non-custodial بالكامل بعد"
  },
  "home.channels.bTag": {
    "de": "Kanal B · Pro+",
    "ru": "Канал B · Pro+",
    "ar": "القناة B · Pro+"
  },
  "home.channels.bTitle": {
    "de": "Self-hosted daemon zero-key",
    "ru": "Self-hosted daemon zero-key",
    "ar": "daemon ذاتي zero-key"
  },
  "home.channels.bLead": {
    "de": "Keys bleiben bei Ihnen — für mehr Kontrolle.",
    "ru": "Ключи у вас — больше контроля.",
    "ar": "المفاتيح لديك — تحكم أعلى."
  },
  "home.channels.bPoint1": {
    "de": "Lokaler / self-hosted daemon",
    "ru": "Локальный / self-hosted daemon",
    "ar": "daemon محلي / ذاتي"
  },
  "home.channels.bPoint2": {
    "de": "Plattform hält keine Trading-Keys",
    "ru": "Платформа не хранит торговые ключи",
    "ar": "المنصة لا تحتفظ بمفاتيح التداول"
  },
  "home.channels.bPoint3": {
    "de": "Cross-Venue, erweitertes Risiko, unbegrenzte Follow-Slots",
    "ru": "Cross-Venue, расширенный риск, безлимит слотов",
    "ar": "Cross-Venue، مخاطر متقدمة، slots غير محدود"
  },
  "home.channels.bCustody": {
    "de": "✓ Zero-Key-Ausführung · Pro+ erforderlich",
    "ru": "✓ Zero-key исполнение · нужен Pro+",
    "ar": "✓ تنفيذ zero-key · يتطلب Pro+"
  },
  "home.closing.title": {
    "de": "Wählen Sie, wem Sie folgen, und kopieren Sie deren Trades.",
    "ru": "Выберите, за кем следовать, и копируйте сделки.",
    "ar": "اختر من تتابع وانسخ صفقاتهم."
  },
  "home.closing.ctaDiscover": {
    "de": "Trader entdecken",
    "ru": "Найти трейдеров",
    "ar": "اكتشف المتداولين"
  },
  "home.closing.ctaConnect": {
    "de": "Wallet verbinden",
    "ru": "Подключить кошелёк",
    "ar": "ربط المحفظة"
  },
  "home.hot.sectionTitle": {
    "de": "Hot Trader",
    "ru": "Горячие трейдеры",
    "ar": "متداولون رائجون"
  },
  "home.hot.sectionDesc": {
    "de": "Vorschau sortiert nach 30-Tage-ROI.",
    "ru": "Превью по ROI за 30 дней.",
    "ar": "معاينة حسب ROI 30 يوماً."
  },
  "home.hot.viewLeaderboard": {
    "de": "Volle Rangliste →",
    "ru": "Полный рейтинг →",
    "ar": "لوحة المتصدرين الكاملة →"
  },
  "home.hot.emptyTitle": {
    "de": "Noch keine Hot Trader",
    "ru": "Нет hot трейдеров",
    "ar": "لا متداولين رائجين"
  },
  "home.hot.emptyAction": {
    "de": "Rangliste ansehen →",
    "ru": "Смотреть рейтинг →",
    "ar": "تصفح لوحة المتصدرين →"
  },
  "home.hot.colTrader": {
    "de": "Trader",
    "ru": "Трейдер",
    "ar": "المتداول"
  },
  "home.hot.colPlatform": {
    "de": "Venue",
    "ru": "Venue",
    "ar": "Venue"
  },
  "home.hot.colWinRate": {
    "de": "Gewinnrate",
    "ru": "Винрейт",
    "ar": "نسبة الفوز"
  },
  "home.hot.loadError": {
    "de": "Hot-Liste konnte nicht geladen werden",
    "ru": "Не удалось загрузить hot-список",
    "ar": "تعذّر تحميل القائمة الرائجة"
  },
  "leaderboard.title": {
    "de": "Rangliste",
    "ru": "Рейтинг",
    "ar": "لوحة المتصدرين"
  },
  "leaderboard.searchPlaceholder": {
    "de": "Adresse / Alias / @x",
    "ru": "Адрес / alias / @x",
    "ar": "عنوان / alias / @x"
  },
  "leaderboard.allPlatforms": {
    "de": "Alle Plattformen",
    "ru": "Все платформы",
    "ar": "جميع المنصات"
  },
  "leaderboard.allPlatformsShort": {
    "de": "Alle Plattformen",
    "ru": "Все",
    "ar": "الكل"
  },
  "leaderboard.sortDesc": {
    "de": "Absteigend",
    "ru": "По убыванию",
    "ar": "تنازلي"
  },
  "leaderboard.andFilters": {
    "de": "Kombinierte Filter",
    "ru": "Комбинированные фильтры",
    "ar": "فلاتر مجمعة"
  },
  "leaderboard.hotOnly": {
    "de": "Nur Hot",
    "ru": "Только hot",
    "ar": "Hot فقط"
  },
  "leaderboard.verifiedOnly": {
    "de": "Nur verifiziert",
    "ru": "Только verified",
    "ar": "موثق فقط"
  },
  "leaderboard.hideBots": {
    "de": "Bots ausblenden",
    "ru": "Скрыть ботов",
    "ar": "إخفاء البots"
  },
  "leaderboard.requirePerf": {
    "de": "Strikter Perioden/Kategorie-Abgleich",
    "ru": "Строгое совпадение периода/категории",
    "ar": "تطابق صارم للفترة/الفئة"
  },
  "leaderboard.empty": {
    "de": "Keine passenden Trader",
    "ru": "Нет подходящих трейдеров",
    "ar": "لا متداولين مطابقين"
  },
  "leaderboard.emptyStrict": {
    "de": "Keine passenden Trader (Strikter Filter entfernte ohne Performance für diese Periode/Kategorie)",
    "ru": "Нет трейдеров (строгий фильтр убрал без performance за период/категорию)",
    "ar": "لا متداولين (الفلتر الصارم أزال من بلا أداء لهذه الفترة/الفئة)"
  },
  "leaderboard.colRank": {
    "de": "#",
    "ru": "#",
    "ar": "#"
  },
  "leaderboard.colTrader": {
    "de": "Trader",
    "ru": "Трейдер",
    "ar": "المتداول"
  },
  "leaderboard.colSpark": {
    "de": "Chart",
    "ru": "График",
    "ar": "الرسم"
  },
  "leaderboard.colRoi": {
    "de": "ROI",
    "ru": "ROI",
    "ar": "ROI"
  },
  "leaderboard.colSharpe": {
    "de": "Sharpe",
    "ru": "Sharpe",
    "ar": "Sharpe"
  },
  "leaderboard.colWinRate": {
    "de": "Gewinnrate",
    "ru": "Винрейт",
    "ar": "نسبة الفوز"
  },
  "leaderboard.colDrawdown": {
    "de": "Drawdown",
    "ru": "Просадка",
    "ar": "الانخفاض"
  },
  "leaderboard.colPnl": {
    "de": "Realisiert",
    "ru": "Реализовано",
    "ar": "محقق"
  },
  "leaderboard.colPlatform": {
    "de": "Venue",
    "ru": "Venue",
    "ar": "Venue"
  },
  "leaderboard.colTags": {
    "de": "Tags",
    "ru": "Теги",
    "ar": "الوسوم"
  },
  "leaderboard.colBot": {
    "de": "Bot",
    "ru": "Bot",
    "ar": "Bot"
  },
  "leaderboard.colWatch": {
    "de": "Beobachten",
    "ru": "Наблюдать",
    "ar": "مراقبة"
  },
  "leaderboard.watchTitle": {
    "de": "Zur Watchlist hinzufügen",
    "ru": "В список наблюдения",
    "ar": "إضافة لقائمة المراقبة"
  },
  "leaderboard.watchAdded": {
    "de": "Zur Watchlist hinzugefügt",
    "ru": "Добавлено в watchlist",
    "ar": "أُضيف لقائمة المراقبة"
  },
  "leaderboard.watchExists": {
    "de": "Bereits auf Watchlist",
    "ru": "Уже в watchlist",
    "ar": "موجود في قائمة المراقبة"
  },
  "leaderboard.watchFailed": {
    "de": "Watchlist-Hinzufügen fehlgeschlagen",
    "ru": "Не удалось добавить в watchlist",
    "ar": "فشل الإضافة لقائمة المراقبة"
  },
  "leaderboard.botMarked": {
    "de": "Von botfilter als Bot markiert",
    "ru": "Помечен botfilter как bot",
    "ar": "مُعلَّم botfilter كبوت"
  },
  "leaderboard.botConfidence": {
    "de": "botfilter-Konfidenz",
    "ru": "Уверенность botfilter",
    "ar": "ثقة botfilter"
  },
  "leaderboard.hotMark": {
    "de": "Hot-Key",
    "ru": "Hot key",
    "ar": "مفتاح hot"
  },
  "leaderboard.verifiedMark": {
    "de": "Verified",
    "ru": "Verified",
    "ar": "موثق"
  },
  "leaderboard.showing": {
    "de": "Zeige {start}-{end} / {total}",
    "ru": "Показано {start}-{end} / {total}",
    "ar": "عرض {start}-{end} / {total}"
  },
  "leaderboard.showingPartial": {
    "de": "Zeige {start}-{end}",
    "ru": "Показано {start}-{end}",
    "ar": "عرض {start}-{end}"
  },
  "leaderboard.lastPage": {
    "de": " (letzte Seite)",
    "ru": " (последняя)",
    "ar": " (الصفحة الأخيرة)"
  },
  "leaderboard.jumpTo": {
    "de": "Gehe zu",
    "ru": "Перейти",
    "ar": "انتقل إلى"
  },
  "leaderboard.page": {
    "de": "Seite",
    "ru": "стр.",
    "ar": "صفحة"
  },
  "leaderboard.jump": {
    "de": "Los",
    "ru": "Перейти",
    "ar": "انتقال"
  },
  "leaderboard.pageLabel": {
    "de": "Seitennummer",
    "ru": "Номер страницы",
    "ar": "رقم الصفحة"
  },
  "leaderboard.searchTerm": {
    "de": "Suche „{q}\"",
    "ru": "Поиск «{q}»",
    "ar": "بحث «{q}»"
  },
  "leaderboard.peopleCount": {
    "de": " · {n} Trader",
    "ru": " · {n} трейдеров",
    "ar": " · {n} متداول"
  },
  "leaderboard.sort.roi": {
    "de": "ROI",
    "ru": "ROI",
    "ar": "ROI"
  },
  "leaderboard.sort.sharpe": {
    "de": "Sharpe",
    "ru": "Sharpe",
    "ar": "Sharpe"
  },
  "leaderboard.sort.win_rate": {
    "de": "Gewinnrate",
    "ru": "Винрейт",
    "ar": "نسبة الفوز"
  },
  "leaderboard.sort.max_drawdown": {
    "de": "Drawdown",
    "ru": "Просадка",
    "ar": "الانخفاض"
  },
  "leaderboard.sort.realized_pnl": {
    "de": "Realisierter PnL",
    "ru": "Реализованный PnL",
    "ar": "PnL محقق"
  },
  "leaderboard.sort.total_volume": {
    "de": "Volumen",
    "ru": "Объём",
    "ar": "الحجم"
  },
  "leaderboard.sort.updated_at": {
    "de": "Aktualisiert",
    "ru": "Обновлено",
    "ar": "محدّث"
  },
  "leaderboard.period.1d": {
    "de": "1T",
    "ru": "1Д",
    "ar": "يوم"
  },
  "leaderboard.period.1w": {
    "de": "1W",
    "ru": "1Н",
    "ar": "أسبوع"
  },
  "leaderboard.period.1m": {
    "de": "1M",
    "ru": "1М",
    "ar": "شهر"
  },
  "leaderboard.period.1y": {
    "de": "1J",
    "ru": "1Г",
    "ar": "سنة"
  },
  "leaderboard.period.ytd": {
    "de": "YTD",
    "ru": "С нач. года",
    "ar": "منذ بداية العام"
  },
  "leaderboard.period.all": {
    "de": "Alle",
    "ru": "Все",
    "ar": "الكل"
  },
  "leaderboard.category.OVERALL": {
    "de": "Alle",
    "ru": "Все",
    "ar": "الكل"
  },
  "leaderboard.category.POLITICS": {
    "de": "Politik",
    "ru": "Политика",
    "ar": "السياسة"
  },
  "leaderboard.category.SPORTS": {
    "de": "Sport",
    "ru": "Спорт",
    "ar": "الرياضة"
  },
  "leaderboard.category.ESPORTS": {
    "de": "Esports",
    "ru": "Киберспорт",
    "ar": "الرياضات الإلكترونية"
  },
  "leaderboard.category.CRYPTO": {
    "de": "Krypto",
    "ru": "Крипто",
    "ar": "العملات الرقمية"
  },
  "leaderboard.category.CULTURE": {
    "de": "Kultur",
    "ru": "Культура",
    "ar": "الثقافة"
  },
  "leaderboard.category.MENTIONS": {
    "de": "Erwähnungen",
    "ru": "Упоминания",
    "ar": "الإشارات"
  },
  "leaderboard.category.WEATHER": {
    "de": "Wetter",
    "ru": "Погода",
    "ar": "الطقس"
  },
  "leaderboard.category.ECONOMICS": {
    "de": "Wirtschaft",
    "ru": "Экономика",
    "ar": "الاقتصاد"
  },
  "leaderboard.category.TECH": {
    "de": "Tech",
    "ru": "Тех",
    "ar": "التقنية"
  },
  "leaderboard.category.FINANCE": {
    "de": "Finanzen",
    "ru": "Финансы",
    "ar": "المالية"
  },
  "errors.network": {
    "de": "Netzwerkfehler: {message}",
    "ru": "Сетевая ошибка: {message}",
    "ar": "خطأ شبكة: {message}"
  },
  "errors.sessionExpired": {
    "de": "Sitzung abgelaufen. Bitte Wallet erneut verbinden",
    "ru": "Сессия истекла. Переподключите кошелёк",
    "ar": "انتهت الجلسة. أعد ربط المحفظة"
  },
  "errors.serviceUnavailable": {
    "de": "Dienst vorübergehend nicht verfügbar. Bitte später erneut versuchen",
    "ru": "Сервис временно недоступен. Попробуйте позже",
    "ar": "الخدمة غير متاحة مؤقتاً. حاول لاحقاً"
  },
  "walletConnect.ariaLabel": {
    "de": "Wallet verbinden",
    "ru": "Подключить кошелёк",
    "ar": "ربط المحفظة"
  },
  "walletConnect.title": {
    "de": "Wallet verbinden",
    "ru": "Подключить кошелёк",
    "ar": "ربط المحفظة"
  },
  "walletConnect.hint": {
    "de": "Installierte Browser-Wallet wählen",
    "ru": "Выберите установленный wallet",
    "ar": "اختر محفظة امتداد مثبتة"
  },
  "walletConnect.detecting": {
    "de": "Wallets werden erkannt…",
    "ru": "Поиск wallets…",
    "ar": "جاري اكتشاف المحافظ…"
  },
  "walletConnect.connect": {
    "de": "Verbinden",
    "ru": "Подключить",
    "ar": "ربط"
  },
  "walletConnect.cancel": {
    "de": "Abbrechen",
    "ru": "Отмена",
    "ar": "إلغاء"
  },
  "walletConnect.noWallet": {
    "de": "Keine Browser-Wallet erkannt",
    "ru": "Extension wallet не обнаружен",
    "ar": "لم تُكتشف محفظة امتداد"
  },
  "walletConnect.installHint": {
    "de": "MetaMask / TokenPocket / Rabby / OKX installieren und Seite neu laden",
    "ru": "Установите MetaMask / TokenPocket / Rabby / OKX и обновите",
    "ar": "ثبّت MetaMask / TokenPocket / Rabby / OKX ثم حدّث"
  },
  "walletConnect.unknownWallet": {
    "de": "Unbekannte Wallet",
    "ru": "Неизвестный wallet",
    "ar": "محفظة غير معروفة"
  },
  "walletConnect.connected": {
    "de": "Wallet verbunden",
    "ru": "Кошелёк подключён",
    "ar": "تم ربط المحفظة"
  },
  "siwe.injectedWallet": {
    "de": "Injected wallet",
    "ru": "Injected wallet",
    "ar": "محفظة مدمجة"
  },
  "ui.periodLabel": {
    "de": "Zeitraum",
    "ru": "Период",
    "ar": "الفترة"
  },
  "ui.emptyDefault": {
    "de": "Keine Daten",
    "ru": "Нет данных",
    "ar": "لا بيانات"
  },
  "ui.errorTitle": {
    "de": "Etwas ist schiefgelaufen",
    "ru": "Что-то пошло не так",
    "ar": "حدث خطأ"
  },
  "oneTimeSecret.defaultTitle": {
    "de": "Geheimer Schlüssel (einmalig)",
    "ru": "Секретный ключ (один раз)",
    "ar": "مفتاح سري (يُعرض مرة واحدة)"
  },
  "oneTimeSecret.defaultWarn": {
    "de": "Jetzt speichern. Nach dem Schließen nicht mehr sichtbar.",
    "ru": "Сохраните сейчас. После закрытия не показать.",
    "ar": "احفظه الآن. لن تراه مجدداً بعد الإغلاق."
  },
  "oneTimeSecret.copy": {
    "de": "Kopieren",
    "ru": "Копировать",
    "ar": "نسخ"
  },
  "oneTimeSecret.copied": {
    "de": "Kopiert",
    "ru": "Скопировано",
    "ar": "تم النسخ"
  },
  "oneTimeSecret.confirmSaved": {
    "de": "Ich habe diesen Schlüssel sicher gespeichert",
    "ru": "Я сохранил этот ключ",
    "ar": "حفظت هذا المفتاح بأمان"
  },
  "oneTimeSecret.close": {
    "de": "Schließen",
    "ru": "Закрыть",
    "ar": "إغلاق"
  },
  "settings.pageTitle": {
    "de": "Kontoeinstellungen",
    "ru": "Настройки аккаунта",
    "ar": "إعدادات الحساب"
  },
  "settings.pageSubtitle": {
    "de": "Abo, Credentials und Copy-Kanäle",
    "ru": "Подписка, credentials и каналы копирования",
    "ar": "الاشتراك والاعتمادات وقنوات النسخ"
  },
  "settings.hubSubTitle": {
    "de": "Abo",
    "ru": "Подписка",
    "ar": "الاشتراك"
  },
  "settings.hubSubDesc": {
    "de": "Stufen & Vorteile",
    "ru": "Тарифы и benefits",
    "ar": "المستويات والمزايا"
  },
  "settings.hubCredTitle": {
    "de": "Venue credentials",
    "ru": "Venue credentials",
    "ar": "اعتمادات Venue"
  },
  "settings.hubCredDesc": {
    "de": "Trading-Auth",
    "ru": "Торговая авторизация",
    "ar": "تفويض التداول"
  },
  "settings.hubDelTitle": {
    "de": "Delegation",
    "ru": "Делегирование",
    "ar": "التفويض"
  },
  "settings.hubDelDesc": {
    "de": "Custody & Proxy",
    "ru": "Custody и proxy",
    "ar": "الحفظ والوكالة"
  },
  "settings.hubDaemonDesc": {
    "de": "Self-hosted Kanal",
    "ru": "Self-hosted канал",
    "ar": "قناة ذاتية"
  },
  "settings.accountTitle": {
    "de": "Konto",
    "ru": "Аккаунт",
    "ar": "الحساب"
  },
  "settings.connectedWallet": {
    "de": "Verbundene Wallet:",
    "ru": "Подключённый кошелёк:",
    "ar": "المحفظة المتصلة:"
  },
  "settings.userId": {
    "de": "Benutzer-ID: {id}",
    "ru": "ID пользователя: {id}",
    "ar": "معرف المستخدم: {id}"
  },
  "settings.accountLoadError": {
    "de": "Konto konnte nicht geladen werden: {message}",
    "ru": "Ошибка загрузки аккаунта: {message}",
    "ar": "فشل تحميل الحساب: {message}"
  },
  "settings.subTitle": {
    "de": "Abo",
    "ru": "Подписка",
    "ar": "الاشتراك"
  },
  "settings.currentTier": {
    "de": "Aktuelle Stufe:",
    "ru": "Текущий тариф:",
    "ar": "المستوى الحالي:"
  },
  "settings.subUntil": {
    "de": "Abo bis {date}",
    "ru": "Подписка до {date}",
    "ar": "اشتراك حتى {date}"
  },
  "settings.upgradeLink": {
    "de": "Upgrade / verwalten →",
    "ru": "Upgrade / управление →",
    "ar": "ترقية / إدارة →"
  },
  "settings.subLoadError": {
    "de": "Abo konnte nicht geladen werden: {message}",
    "ru": "Ошибка загрузки подписки: {message}",
    "ar": "فشل تحميل الاشتراك: {message}"
  },
  "settings.credTitle": {
    "de": "Venue credentials",
    "ru": "Venue credentials",
    "ar": "اعتمادات Venue"
  },
  "settings.credEmpty": {
    "de": "Keine Venue-Credentials konfiguriert",
    "ru": "Venue credentials не настроены",
    "ar": "لا اعتمادات Venue"
  },
  "settings.credConfigure": {
    "de": "Konfigurieren →",
    "ru": "Настроить →",
    "ar": "إعداد →"
  },
  "settings.proxyAddress": {
    "de": " · Proxy: {address}",
    "ru": " · Proxy: {address}",
    "ar": " · الوكيل: {address}"
  },
  "settings.credLoadError": {
    "de": "Credentials konnten nicht geladen werden: {message}",
    "ru": "Ошибка загрузки credentials: {message}",
    "ar": "فشل تحميل الاعتمادات: {message}"
  },
  "settings.daemonDesc": {
    "de": "Für self-hosted daemon Copy-Pulls (Kanal B). Rotation invalidiert den alten Key sofort; Klartext wird einmal angezeigt.",
    "ru": "Для self-hosted daemon (Канал B). Ротация аннулирует старый key; plaintext один раз.",
    "ar": "لـ daemon ذاتي (القناة B). التدوير يلغي المفتاح القديم؛ النص مرة واحدة."
  },
  "settings.daemonGoto": {
    "de": "daemon-Key verwalten →",
    "ru": "Управление daemon key →",
    "ar": "إدارة daemon key →"
  },
  "wallet.pageTitle": {
    "de": "Wallet",
    "ru": "Кошелёк",
    "ar": "المحفظة"
  },
  "wallet.rechargeTitle": {
    "de": "Einzahlen",
    "ru": "Депозит",
    "ar": "إيداع"
  },
  "wallet.withdrawTitle": {
    "de": "Auszahlen",
    "ru": "Вывод",
    "ar": "سحب"
  },
  "wallet.withdrawHistoryTitle": {
    "de": "Auszahlungshistorie",
    "ru": "История выводов",
    "ar": "سجل السحب"
  },
  "wallet.redeemTitle": {
    "de": "Einlösen (abgerechnete Märkte)",
    "ru": "Погашение (закрытые рынки)",
    "ar": "استرداد (أسواق مُسوّاة)"
  },
  "wallet.redeemHistoryTitle": {
    "de": "Einlösungshistorie",
    "ru": "История погашений",
    "ar": "سجل الاسترداد"
  },
  "wallet.balanceLabel": {
    "de": "Verfügbares Guthaben (pUSD)",
    "ru": "Доступный баланс (pUSD)",
    "ar": "الرصيد المتاح (pUSD)"
  },
  "wallet.refreshBalance": {
    "de": "🔄 Guthaben aktualisieren",
    "ru": "🔄 Обновить баланс",
    "ar": "🔄 تحديث الرصيد"
  },
  "wallet.refreshing": {
    "de": "Guthaben wird aktualisiert…",
    "ru": "Обновление баланса…",
    "ar": "جاري تحديث الرصيد…"
  },
  "wallet.balanceLive": {
    "de": "Live (CLOB /balance-allowance)",
    "ru": "Live (CLOB /balance-allowance)",
    "ar": "Live (CLOB /balance-allowance)"
  },
  "wallet.balanceOffline": {
    "de": "Offline-Provision — Guthaben nicht verfügbar",
    "ru": "Offline provision — баланс недоступен",
    "ar": "Offline provision — الرصيد غير متاح"
  },
  "wallet.depositAddressTitle": {
    "de": "Deposit Wallet-Adresse",
    "ru": "Адрес Deposit Wallet",
    "ar": "عنوان Deposit Wallet"
  },
  "wallet.copyDeposit": {
    "de": "Einzahlungsadresse kopieren",
    "ru": "Копировать адрес депозита",
    "ar": "نسخ عنوان الإيداع"
  },
  "wallet.depositHint": {
    "de": "pUSD auf Polygon an diese Adresse senden. Nach Ankunft Guthaben aktualisieren.",
    "ru": "Отправьте pUSD на Polygon на этот адрес. Обновите баланс после зачисления.",
    "ar": "أرسل pUSD على Polygon إلى هذا العنوان. حدّث الرصيد بعد الوصول."
  },
  "wallet.noDepositWallet": {
    "de": "Deposit Wallet noch nicht bereitgestellt (zu Delegation)",
    "ru": "Deposit Wallet не provisioned (перейдите в Delegation)",
    "ar": "Deposit Wallet غير مُجهّز (اذهب إلى Delegation)"
  },
  "wallet.contractTitle": {
    "de": "pUSD-Vertrag (Polygon)",
    "ru": "Контракт pUSD (Polygon)",
    "ar": "عقد pUSD (Polygon)"
  },
  "wallet.copyContract": {
    "de": "Vertragsadresse kopieren",
    "ru": "Копировать адрес контракта",
    "ar": "نسخ عنوان العقد"
  },
  "wallet.contractHint": {
    "de": "Polygon Mainnet (Chain ID 137). Beim Hinzufügen eines Custom Tokens verwenden — nicht auf anderen Chains senden.",
    "ru": "Polygon mainnet (Chain ID 137). Для добавления токена — не отправляйте на других сетях.",
    "ar": "Polygon mainnet (Chain ID 137). عند إضافة رمز مخصص — لا ترسل على شبكات أخرى."
  },
  "wallet.noCredential": {
    "de": "Polymarket-Delegation nicht bereitgestellt — Ein-/Auszahlung nicht verfügbar",
    "ru": "Polymarket delegation не provisioned — депозит/вывод недоступен",
    "ar": "Polymarket delegation غير مُجهّز — الإيداع/السحب غير متاح"
  },
  "wallet.gotoDelegation": {
    "de": "Zur Delegation →",
    "ru": "Перейти в Delegation →",
    "ar": "إلى Delegation →"
  },
  "wallet.loadError": {
    "de": "Laden fehlgeschlagen: {message}",
    "ru": "Ошибка загрузки: {message}",
    "ar": "فشل التحميل: {message}"
  },
  "wallet.withdrawIntro": {
    "de": "pUSD an gebundene Wallet-Adresse auszahlen. Adressen unter Einstellungen → Wallet binden.",
    "ru": "Вывод pUSD на привязанный адрес. Привяжите адреса в Настройки → Wallet.",
    "ar": "اسحب pUSD إلى عنوان مرتبط. اربط العناوين في الإعدادات → Wallet."
  },
  "wallet.selectWallet": {
    "de": "Gebundene Wallet wählen…",
    "ru": "Выберите привязанный кошелёк…",
    "ar": "اختر محفظة مرتبطة…"
  },
  "wallet.withdrawTo": {
    "de": "Auszahlen an",
    "ru": "Вывод на",
    "ar": "السحب إلى"
  },
  "wallet.amountLabel": {
    "de": "Betrag (pUSD)",
    "ru": "Сумма (pUSD)",
    "ar": "المبلغ (pUSD)"
  },
  "wallet.amountPlaceholder": {
    "de": "Betrag (pUSD)",
    "ru": "Сумма (pUSD)",
    "ar": "المبلغ (pUSD)"
  },
  "wallet.balanceUnknown": {
    "de": "Guthaben nicht verfügbar (Offline-Provision oder Abruf fehlgeschlagen)",
    "ru": "Баланс недоступен (offline provision или ошибка)",
    "ar": "الرصيد غير متاح (offline provision أو فشل الجلب)"
  },
  "wallet.availableBalance": {
    "de": "Verfügbar: {balance}",
    "ru": "Доступно: {balance}",
    "ar": "متاح: {balance}"
  },
  "wallet.offlineWarning": {
    "de": "⚠ Offline-Provision kann nicht auszahlen (Online-Provision + pUSD-Einzahlung nötig)",
    "ru": "⚠ Offline provision не может выводить (нужен online provision + pUSD)",
    "ar": "⚠ Offline provision لا يسمح بالسحب (يلزم online provision + pUSD)"
  },
  "wallet.toastSelectAddress": {
    "de": "Auszahlungsadresse wählen",
    "ru": "Выберите адрес вывода",
    "ar": "اختر عنوان السحب"
  },
  "wallet.toastInvalidAmount": {
    "de": "Gültigen Betrag eingeben",
    "ru": "Введите корректную сумму",
    "ar": "أدخل مبلغاً صالحاً"
  },
  "wallet.toastExceedsBalance": {
    "de": "Betrag übersteigt verfügbares Guthaben {balance}",
    "ru": "Сумма превышает доступный баланс {balance}",
    "ar": "المبلغ يتجاوز الرصيد المتاح {balance}"
  },
  "wallet.confirmWithdraw": {
    "de": "Auszahlung {amount} pUSD an\n{addressPrefix}…{addressSuffix}?\n\nPlattform signiert On-Chain-Transfer. Unwiderruflich.",
    "ru": "Вывести {amount} pUSD на\n{addressPrefix}…{addressSuffix}?\n\nПлатформа подпишет on-chain transfer. Нельзя отменить.",
    "ar": "سحب {amount} pUSD إلى\n{addressPrefix}…{addressSuffix}؟\n\nالمنصة ستوقّع تحويل on-chain. لا يمكن التراجع."
  },
  "wallet.toastWithdrawSuccess": {
    "de": "Auszahlung erfolgreich, tx {txHash}",
    "ru": "Вывод успешен, tx {txHash}",
    "ar": "نجح السحب، tx {txHash}"
  },
  "wallet.toastWithdrawPending": {
    "de": "Auszahlung eingereicht — warte auf Bestätigung (Historie prüfen)",
    "ru": "Вывод отправлен — ожидание подтверждения (см. историю)",
    "ar": "تم إرسال السحب — انتظر التأكيد (راجع السجل)"
  },
  "wallet.toastWithdrawFailed": {
    "de": "Auszahlung fehlgeschlagen: {reason}",
    "ru": "Вывод не удался: {reason}",
    "ar": "فشل السحب: {reason}"
  },
  "wallet.toastWithdrawError": {
    "de": "Auszahlung fehlgeschlagen",
    "ru": "Вывод не удался",
    "ar": "فشل السحب"
  },
  "wallet.unknownReason": {
    "de": "Unbekannter Grund",
    "ru": "Неизвестная причина",
    "ar": "سبب غير معروف"
  },
  "wallet.withdrawEmpty": {
    "de": "Keine Auszahlungshistorie",
    "ru": "Нет истории выводов",
    "ar": "لا سجل سحب"
  },
  "wallet.colTime": {
    "de": "Zeit",
    "ru": "Время",
    "ar": "الوقت"
  },
  "wallet.colToAddress": {
    "de": "An",
    "ru": "Куда",
    "ar": "إلى"
  },
  "wallet.colAmount": {
    "de": "Betrag",
    "ru": "Сумма",
    "ar": "المبلغ"
  },
  "wallet.colStatus": {
    "de": "Status",
    "ru": "Статус",
    "ar": "الحالة"
  },
  "wallet.colTxHash": {
    "de": "Tx-Hash",
    "ru": "Хеш tx",
    "ar": "تجزئة المعاملة"
  },
  "wallet.colNote": {
    "de": "Notiz",
    "ru": "Примечание",
    "ar": "ملاحظة"
  },
  "wallet.withdrawHistoryError": {
    "de": "Auszahlungshistorie konnte nicht geladen werden: {message}",
    "ru": "Ошибка загрузки истории выводов: {message}",
    "ar": "فشل تحميل سجل السحب: {message}"
  },
  "wallet.redeemEmpty": {
    "de": "Keine einlösbaren Positionen (Gewinner auf abgerechneten Märkten werden automatisch eingelöst)",
    "ru": "Нет позиций для погашения (выигрышные на закрытых рынках погашаются автоматически)",
    "ar": "لا مراكز قابلة للاسترداد (الفائزة في الأسواق المُسوّاة تُسترد تلقائياً)"
  },
  "wallet.redeemIntro": {
    "de": "Gewinner auf abgerechneten Märkten werden 1:1 in pUSD eingelöst. Auto-Einlösung läuft periodisch; manuell jetzt möglich.",
    "ru": "Выигрышные на закрытых рынках погашаются 1:1 в pUSD. Auto-redeem периодически; можно вручную.",
    "ar": "المراكز الفائزة في الأسواق المُسوّاة تُسترد 1:1 إلى pUSD. الاسترداد التلقائي دوري؛ يمكن الآن يدوياً."
  },
  "wallet.colMarket": {
    "de": "Markt",
    "ru": "Рынок",
    "ar": "السوق"
  },
  "wallet.colOutcome": {
    "de": "Ergebnis",
    "ru": "Исход",
    "ar": "النتيجة"
  },
  "wallet.colRedeemable": {
    "de": "Einlösbar",
    "ru": "Доступно к погашению",
    "ar": "قابل للاسترداد"
  },
  "wallet.colEstimated": {
    "de": "Gesch. Auszahlung",
    "ru": "Ожид. выплата",
    "ar": "الدفع المتوقع"
  },
  "wallet.colAction": {
    "de": "Aktion",
    "ru": "Действие",
    "ar": "إجراء"
  },
  "wallet.redeemInProgress": {
    "de": "Einlösung / erledigt",
    "ru": "Погашение / готово",
    "ar": "استرداد / تم"
  },
  "wallet.redeem": {
    "de": "Einlösen",
    "ru": "Погасить",
    "ar": "استرداد"
  },
  "wallet.confirmRedeem": {
    "de": "Gewinnposition für diesen Markt einlösen?\n\nPlattform signiert On-Chain-Einlösung; pUSD geht an Ihre Deposit Wallet.",
    "ru": "Погасить выигрышную позицию?\n\nПлатформа подпишет on-chain redeem; pUSD на Deposit Wallet.",
    "ar": "استرداد المركز الفائز؟\n\nالمنصة ستوقّع redeem on-chain؛ pUSD إلى Deposit Wallet."
  },
  "wallet.redeeming": {
    "de": "Einlösung…",
    "ru": "Погашение…",
    "ar": "جاري الاسترداد…"
  },
  "wallet.toastRedeemSuccess": {
    "de": "Eingelöst — ~{amount} pUSD gutgeschrieben ({txHash})",
    "ru": "Погашено — ~{amount} pUSD зачислено ({txHash})",
    "ar": "تم الاسترداد — ~{amount} pUSD ({txHash})"
  },
  "wallet.toastRedeemPending": {
    "de": "Einlösung eingereicht — warte auf Bestätigung",
    "ru": "Погашение отправлено — ожидание подтверждения",
    "ar": "تم إرسال الاسترداد — انتظر التأكيد"
  },
  "wallet.toastRedeemFailed": {
    "de": "Einlösung fehlgeschlagen: {reason}",
    "ru": "Погашение не удалось: {reason}",
    "ar": "فشل الاسترداد: {reason}"
  },
  "wallet.toastRedeemError": {
    "de": "Einlösung fehlgeschlagen",
    "ru": "Погашение не удалось",
    "ar": "فشل الاسترداد"
  },
  "wallet.redeemNoCredential": {
    "de": "Polymarket-Delegation nicht bereitgestellt — Einlösung nicht verfügbar",
    "ru": "Polymarket delegation не provisioned — погашение недоступно",
    "ar": "Polymarket delegation غير مُجهّز — الاسترداد غير متاح"
  },
  "wallet.redeemLoadError": {
    "de": "Einlösbare Liste konnte nicht geladen werden: {message}",
    "ru": "Ошибка загрузки списка погашения: {message}",
    "ar": "فشل تحميل قائمة الاسترداد: {message}"
  },
  "wallet.redeemHistoryEmpty": {
    "de": "Keine Einlösungshistorie",
    "ru": "Нет истории погашений",
    "ar": "لا سجل استرداد"
  },
  "wallet.colSource": {
    "de": "Quelle",
    "ru": "Источник",
    "ar": "المصدر"
  },
  "wallet.sourceAuto": {
    "de": "Auto",
    "ru": "Авто",
    "ar": "تلقائي"
  },
  "wallet.sourceManual": {
    "de": "Manuell",
    "ru": "Вручную",
    "ar": "يدوي"
  },
  "wallet.colSize": {
    "de": "Größe",
    "ru": "Размер",
    "ar": "الحجم"
  },
  "wallet.redeemHistoryError": {
    "de": "Einlösungshistorie konnte nicht geladen werden: {message}",
    "ru": "Ошибка загрузки истории погашений: {message}",
    "ar": "فشل تحميل سجل الاسترداد: {message}"
  },
  "follows.pageTitle": {
    "de": "Meine Follows",
    "ru": "Мои подписки",
    "ar": "متابعاتي"
  },
  "follows.pageTitleCount": {
    "de": "Meine Follows ({count})",
    "ru": "Мои подписки ({count})",
    "ar": "متابعاتي ({count})"
  },
  "follows.create": {
    "de": "+ Neuer Follow",
    "ru": "+ Новая подписка",
    "ar": "+ متابعة جديدة"
  },
  "follows.filter": {
    "de": "Filter",
    "ru": "Фильтр",
    "ar": "تصفية"
  },
  "follows.filterAll": {
    "de": "Alle",
    "ru": "Все",
    "ar": "الكل"
  },
  "follows.filterActive": {
    "de": "Aktiv",
    "ru": "Активные",
    "ar": "نشط"
  },
  "follows.filterPaused": {
    "de": "Pausiert",
    "ru": "На паузе",
    "ar": "متوقف"
  },
  "follows.filterError": {
    "de": "Fehler",
    "ru": "Ошибка",
    "ar": "خطأ"
  },
  "follows.sort": {
    "de": "Sortierung",
    "ru": "Сортировка",
    "ar": "الترتيب"
  },
  "follows.sortCreatedDesc": {
    "de": "Erstellt (neu → alt)",
    "ru": "Создано (новые → старые)",
    "ar": "تاريخ الإنشاء (جديد → قديم)"
  },
  "follows.sortCreatedAsc": {
    "de": "Erstellt (alt → neu)",
    "ru": "Создано (старые → новые)",
    "ar": "تاريخ الإنشاء (قديم → جديد)"
  },
  "follows.loadError": {
    "de": "Laden fehlgeschlagen: {message}",
    "ru": "Ошибка загрузки: {message}",
    "ar": "فشل التحميل: {message}"
  },
  "follows.empty": {
    "de": "Noch niemandem gefolgt",
    "ru": "Пока ни на кого не подписаны",
    "ar": "لا متابعات بعد"
  },
  "follows.emptyAction": {
    "de": "Trader entdecken →",
    "ru": "Найти трейдеров →",
    "ar": "اكتشف المتداولين →"
  },
  "follows.identityLabel": {
    "de": "Identität {idPrefix}…",
    "ru": "Идентичность {idPrefix}…",
    "ar": "هوية {idPrefix}…"
  },
  "follows.metaAddress": {
    "de": "Adresse",
    "ru": "Адрес",
    "ar": "العنوان"
  },
  "follows.metaChannel": {
    "de": "Kanal",
    "ru": "Канал",
    "ar": "القناة"
  },
  "follows.metaExecute": {
    "de": "Ausführung",
    "ru": "Исполнение",
    "ar": "التنفيذ"
  },
  "follows.metaCreated": {
    "de": "Erstellt",
    "ru": "Создано",
    "ar": "تاريخ الإنشاء"
  },
  "follows.metaDailyMax": {
    "de": "Tagesmaximum",
    "ru": "Дневной лимит",
    "ar": "الحد اليومي"
  },
  "follows.metaMaxOpen": {
    "de": "Max. offen",
    "ru": "Макс. открытых",
    "ar": "الحد الأقصى المفتوح"
  },
  "follows.pause": {
    "de": "⏸ Pause",
    "ru": "⏸ Пауза",
    "ar": "⏸ إيقاف"
  },
  "follows.resume": {
    "de": "▶ Fortsetzen",
    "ru": "▶ Возобновить",
    "ar": "▶ استئناف"
  },
  "follows.toastPaused": {
    "de": "Pausiert",
    "ru": "На паузе",
    "ar": "متوقف"
  },
  "follows.toastResumed": {
    "de": "Aktiv",
    "ru": "Активно",
    "ar": "نشط"
  },
  "follows.edit": {
    "de": "✎ Bearbeiten",
    "ru": "✎ Редактировать",
    "ar": "✎ تعديل"
  },
  "follows.toastUpdated": {
    "de": "Aktualisiert",
    "ru": "Обновлено",
    "ar": "تم التحديث"
  },
  "follows.copyId": {
    "de": "ID kopieren",
    "ru": "Копировать ID",
    "ar": "نسخ ID"
  },
  "follows.toastIdCopied": {
    "de": "ID kopiert",
    "ru": "ID скопирован",
    "ar": "تم نسخ ID"
  },
  "follows.delete": {
    "de": "🗑 Löschen",
    "ru": "🗑 Удалить",
    "ar": "🗑 حذف"
  },
  "follows.confirmDelete": {
    "de": "Diesen Follow löschen?",
    "ru": "Удалить эту подписку?",
    "ar": "حذف هذه المتابعة؟"
  },
  "follows.toastDeleted": {
    "de": "Gelöscht",
    "ru": "Удалено",
    "ar": "تم الحذف"
  },
  "follows.channelTg": {
    "de": "Kanal A (tg)",
    "ru": "Канал A (tg)",
    "ar": "القناة A (tg)"
  },
  "follows.channelDaemon": {
    "de": "Kanal B (daemon)",
    "ru": "Канал B (daemon)",
    "ar": "القناة B (daemon)"
  },
  "follows.slotsPro": {
    "de": "Pro+-Slots {used} / ∞",
    "ru": "Pro+ слотов {used} / ∞",
    "ar": "Pro+ slots {used} / ∞"
  },
  "follows.slotsProBadge": {
    "de": "Pro+ aktiv",
    "ru": "Pro+ активен",
    "ar": "Pro+ نشط"
  },
  "follows.slotsFree": {
    "de": "Follow-Slots {used} / {limit} belegt",
    "ru": "Слотов подписок {used} / {limit}",
    "ar": "slots المتابعة {used} / {limit}"
  },
  "follows.upgradePro": {
    "de": "Auf Pro+ upgraden",
    "ru": "Upgrade Pro+",
    "ar": "ترقية Pro+"
  },
  "follows.newTitle": {
    "de": "Neuer Follow",
    "ru": "Новая подписка",
    "ar": "متابعة جديدة"
  },
  "follows.targetTrader": {
    "de": " Single-Venue-Trader",
    "ru": " Трейдер одного Venue",
    "ar": " متداول Venue واحد"
  },
  "follows.targetIdentity": {
    "de": " Cross-Venue-Identität",
    "ru": " Cross-Venue идентичность",
    "ar": " هوية Cross-Venue"
  },
  "follows.platform": {
    "de": "Plattform",
    "ru": "Платформа",
    "ar": "المنصة"
  },
  "follows.address": {
    "de": "Trader-Adresse",
    "ru": "Адрес трейдера",
    "ar": "عنوان المتداول"
  },
  "follows.identityHint": {
    "de": "Nur manuell verifizierte Identitäten werden angezeigt.",
    "ru": "Показаны только manually verified идентичности.",
    "ar": "تُعرض الهويات الم verified يدوياً فقط."
  },
  "follows.identity": {
    "de": "Identität",
    "ru": "Идентичность",
    "ar": "الهوية"
  },
  "follows.noIdentities": {
    "de": "Keine verifizierten Identitäten",
    "ru": "Нет verified идентичностей",
    "ar": "لا هويات verified"
  },
  "follows.noIdentitiesHint": {
    "de": "Noch keine manual_verified Identitäten. Review im Admin abschließen oder Single-Venue-Trader nutzen.",
    "ru": "Нет manual_verified идентичностей. Завершите review в admin или используйте single-venue трейдера.",
    "ar": "لا هويات manual_verified. أكمل المراجعة في admin أو استخدم متداول Venue واحد."
  },
  "follows.selectIdentity": {
    "de": "Identität wählen…",
    "ru": "Выберите идентичность…",
    "ar": "اختر هوية…"
  },
  "follows.unnamed": {
    "de": "Unbenannt",
    "ru": "Без имени",
    "ar": "بدون اسم"
  },
  "follows.identityLoadFailed": {
    "de": "Laden fehlgeschlagen",
    "ru": "Ошибка загрузки",
    "ar": "فشل التحميل"
  },
  "follows.identityLoadFailedHint": {
    "de": "Identitäten konnten nicht geladen werden: {message}. Zu Single-Venue-Trader wechseln.",
    "ru": "Ошибка загрузки идентичностей: {message}. Переключитесь на single-venue трейдера.",
    "ar": "فشل تحميل الهويات: {message}. انتقل إلى متداول Venue واحد."
  },
  "follows.execConfig": {
    "de": "Ausführungskonfiguration",
    "ru": "Конфигурация исполнения",
    "ar": "إعداد التنفيذ"
  },
  "follows.executeVenue": {
    "de": "Ausführungs-Venue",
    "ru": "Venue исполнения",
    "ar": "Venue التنفيذ"
  },
  "follows.channel": {
    "de": "Kanal",
    "ru": "Канал",
    "ar": "القناة"
  },
  "follows.channelTgOpt": {
    "de": "TG Deposit Wallet (Plattform signiert)",
    "ru": "TG Deposit Wallet (платформа подписывает)",
    "ar": "TG Deposit Wallet (المنصة توقّع)"
  },
  "follows.channelDaemonOpt": {
    "de": "Self-hosted daemon (Pro+)",
    "ru": "Self-hosted daemon (Pro+)",
    "ar": "Self-hosted daemon (Pro+)"
  },
  "follows.sizingFixed": {
    "de": "fixed · fester USDC-Betrag pro Trade",
    "ru": "fixed · фиксированный USDC за сделку",
    "ar": "fixed · USDC ثابت لكل صفقة"
  },
  "follows.sizingProportional": {
    "de": "proportional · nach Verhältnis",
    "ru": "proportional · по коэффициенту",
    "ar": "proportional · بنسبة"
  },
  "follows.sizingPercent": {
    "de": "percent_of_balance · % des Guthabens",
    "ru": "percent_of_balance · % баланса",
    "ar": "percent_of_balance · % من الرصيد"
  },
  "follows.sameVenueOnly": {
    "de": " same_venue_only (nur gleicher Venue; aus = Cross-Venue braucht Pro+)",
    "ru": " same_venue_only (только тот же Venue; выкл = cross-venue нужен Pro+)",
    "ar": " same_venue_only (نفس Venue فقط؛ إيقاف = cross-venue يتطلب Pro+)"
  },
  "follows.advanced": {
    "de": "Erweitertes Risiko (optional)",
    "ru": "Расширенный риск (опционально)",
    "ar": "مخاطر متقدمة (اختياري)"
  },
  "follows.maxOrder": {
    "de": "Max. Order-notional (USDC, 0=unbegrenzt)",
    "ru": "Макс. notional ордера (USDC, 0=без лимита)",
    "ar": "الحد الأقصى notional (USDC، 0=غير محدود)"
  },
  "follows.placeholderUnlimited": {
    "de": "Leer = unbegrenzt",
    "ru": "Пусто = без лимита",
    "ar": "فارغ = غير محدود"
  },
  "follows.dailyMax": {
    "de": "Tagesmaximum (USDC, 0=unbegrenzt)",
    "ru": "Дневной лимит (USDC, 0=без лимита)",
    "ar": "الحد اليومي (USDC، 0=غير محدود)"
  },
  "follows.maxOpen": {
    "de": "Max. offene Positionen (0=unbegrenzt)",
    "ru": "Макс. открытых позиций (0=без лимита)",
    "ar": "الحد الأقصى للمراكز (0=غير محدود)"
  },
  "follows.submit": {
    "de": "Follow erstellen",
    "ru": "Создать подписку",
    "ar": "إنشاء متابعة"
  },
  "follows.errorSizing": {
    "de": "sizing-Wert muss > 0 sein",
    "ru": "Значение sizing должно быть > 0",
    "ar": "قيمة sizing يجب أن تكون > 0"
  },
  "follows.errorIdentity": {
    "de": "Verifizierte Identität wählen",
    "ru": "Выберите verified идентичность",
    "ar": "اختر هوية verified"
  },
  "follows.errorPlatformAddress": {
    "de": "Plattform und Adresse eingeben",
    "ru": "Введите платформу и адрес",
    "ar": "أدخل المنصة والعنوان"
  },
  "follows.toastCreated": {
    "de": "Follow erstellt",
    "ru": "Подписка создана",
    "ar": "تم إنشاء المتابعة"
  },
  "dashboard.pageTitle": {
    "de": "Dashboard",
    "ru": "Дашборд",
    "ar": "لوحة التحكم"
  },
  "dashboard.overview": {
    "de": "Übersicht",
    "ru": "Обзор",
    "ar": "نظرة عامة"
  },
  "dashboard.jurisdictionVenues": {
    "de": "Jurisdiktion {jurisdiction} · Venues: {venues}",
    "ru": "Юрисдикция {jurisdiction} · venues: {venues}",
    "ar": "الاختصاص {jurisdiction} · venues: {venues}"
  },
  "dashboard.activeFollows": {
    "de": "Aktive Follows",
    "ru": "Активные подписки",
    "ar": "متابعات نشطة"
  },
  "dashboard.watchlistCount": {
    "de": "Watchlist",
    "ru": "Список наблюдения",
    "ar": "قائمة المراقبة"
  },
  "dashboard.totalCopyOrders": {
    "de": "Copy-Orders",
    "ru": "Copy orders",
    "ar": "أوامر النسخ"
  },
  "dashboard.totalExecutions": {
    "de": "Ausführungen",
    "ru": "Исполнения",
    "ar": "عمليات التنفيذ"
  },
  "dashboard.totalPnl": {
    "de": "Gesamt-PnL",
    "ru": "Общий PnL",
    "ar": "إجمالي PnL"
  },
  "dashboard.bffFallback": {
    "de": "Dashboard-Aggregat nicht verfügbar ({message}); Fallback-Ansicht",
    "ru": "Агрегат дашборда недоступен ({message}); резервный вид",
    "ar": "تجميع لوحة التحكم غير متاح ({message})؛ عرض احتياطي"
  },
  "dashboard.walletBalance": {
    "de": "Deposit Wallet-Guthaben:",
    "ru": "Баланс Deposit Wallet:",
    "ar": "رصيد Deposit Wallet:"
  },
  "dashboard.gotoWallet": {
    "de": "Ein-/Auszahlen →",
    "ru": "Депозит / вывод →",
    "ar": "إيداع / سحب →"
  },
  "dashboard.walletUnavailable": {
    "de": "Wallet-Guthaben nicht verfügbar (Wallet-Seite)",
    "ru": "Баланс кошелька недоступен (см. Wallet)",
    "ar": "الرصيد غير متاح (صفحة Wallet)"
  },
  "dashboard.portfolioTitle": {
    "de": "Portfolio",
    "ru": "Портфель",
    "ar": "المحفظة"
  },
  "dashboard.kpiTotalPnl": {
    "de": "Gesamt P&L",
    "ru": "Общий P&L",
    "ar": "إجمالي P&L"
  },
  "dashboard.kpiTotalRoi": {
    "de": "Gesamt-ROI",
    "ru": "Общий ROI",
    "ar": "إجمالي ROI"
  },
  "dashboard.kpiOpenMv": {
    "de": "Offener Marktwert",
    "ru": "Рыночная стоимость открытых",
    "ar": "قيمة السوق المفتوحة"
  },
  "dashboard.kpiWinRate": {
    "de": "Gewinnrate",
    "ru": "Винрейт",
    "ar": "نسبة الفوز"
  },
  "dashboard.kpiTradeCount": {
    "de": "Trades",
    "ru": "Сделки",
    "ar": "الصفقات"
  },
  "dashboard.kpiUnrealized": {
    "de": "Unrealisiert",
    "ru": "Нереализовано",
    "ar": "غير محقق"
  },
  "dashboard.needsMarketData": {
    "de": "Marktdaten erforderlich",
    "ru": "Нужны рыночные данные",
    "ar": "يلزم بيانات السوق"
  },
  "dashboard.portfolioError": {
    "de": "Portfolio-Aggregat nicht verfügbar ({message})",
    "ru": "Агрегат портфеля недоступен ({message})",
    "ar": "تجميع المحفظة غير متاح ({message})"
  },
  "dashboard.followsTitle": {
    "de": "Meine Follows",
    "ru": "Мои подписки",
    "ar": "متابعاتي"
  },
  "dashboard.followsEmpty": {
    "de": "Noch keine Follows",
    "ru": "Пока нет подписок",
    "ar": "لا متابعات"
  },
  "dashboard.colTarget": {
    "de": "Ziel",
    "ru": "Цель",
    "ar": "الهدف"
  },
  "dashboard.identityFallback": {
    "de": "Identität",
    "ru": "Идентичность",
    "ar": "الهوية"
  },
  "dashboard.colStatus": {
    "de": "Status",
    "ru": "Статус",
    "ar": "الحالة"
  },
  "dashboard.statusActive": {
    "de": "Aktiv",
    "ru": "Активно",
    "ar": "نشط"
  },
  "dashboard.statusPaused": {
    "de": "Pausiert",
    "ru": "На паузе",
    "ar": "متوقف"
  },
  "dashboard.followsError": {
    "de": "Follows konnten nicht geladen werden: {message}",
    "ru": "Ошибка загрузки подписок: {message}",
    "ar": "فشل تحميل المتابعات: {message}"
  },
  "dashboard.execTitle": {
    "de": "Letzte Copy-Fills",
    "ru": "Последние copy fills",
    "ar": "آخر عمليات نسخ"
  },
  "dashboard.execEmpty": {
    "de": "Noch keine Copy-Fills",
    "ru": "Пока нет copy fills",
    "ar": "لا عمليات نسخ"
  },
  "dashboard.colTime": {
    "de": "Zeit",
    "ru": "Время",
    "ar": "الوقت"
  },
  "dashboard.colSide": {
    "de": "Seite",
    "ru": "Сторона",
    "ar": "الاتجاه"
  },
  "dashboard.colSize": {
    "de": "Größe",
    "ru": "Размер",
    "ar": "الحجم"
  },
  "dashboard.colPrice": {
    "de": "Preis",
    "ru": "Цена",
    "ar": "السعر"
  },
  "dashboard.execError": {
    "de": "Copy-Fills nicht verfügbar ({message})",
    "ru": "Copy fills недоступны ({message})",
    "ar": "عمليات النسخ غير متاحة ({message})"
  },
  "dashboard.ordersTitle": {
    "de": "Letzte Copy-Orders",
    "ru": "Последние copy orders",
    "ar": "آخر أوامر النسخ"
  },
  "dashboard.ordersEmpty": {
    "de": "Noch keine Copy-Orders",
    "ru": "Пока нет copy orders",
    "ar": "لا أوامر نسخ"
  },
  "dashboard.colSkipReason": {
    "de": "Grund",
    "ru": "Причина",
    "ar": "السبب"
  },
  "dashboard.ordersError": {
    "de": "Copy-Orders nicht verfügbar ({message})",
    "ru": "Copy orders недоступны ({message})",
    "ar": "أوامر النسخ غير متاحة ({message})"
  },
  "portfolio.pageTitle": {
    "de": "Portfolio",
    "ru": "Портфель",
    "ar": "المحفظة"
  },
  "portfolio.periodLabel": {
    "de": "Zeitraum",
    "ru": "Период",
    "ar": "الفترة"
  },
  "portfolio.kpiTotalPnl": {
    "de": "Gesamt P&L",
    "ru": "Общий P&L",
    "ar": "إجمالي P&L"
  },
  "portfolio.kpiTotalRoi": {
    "de": "Gesamt-ROI",
    "ru": "Общий ROI",
    "ar": "إجمالي ROI"
  },
  "portfolio.kpiOpenMv": {
    "de": "Offener Marktwert",
    "ru": "Рыночная стоимость открытых",
    "ar": "قيمة السوق المفتوحة"
  },
  "portfolio.kpiWinRate": {
    "de": "Gewinnrate",
    "ru": "Винрейт",
    "ar": "نسبة الفوز"
  },
  "portfolio.kpiTradeCount": {
    "de": "Trades",
    "ru": "Сделки",
    "ar": "الصفقات"
  },
  "portfolio.kpiUnrealized": {
    "de": "Unrealisiert",
    "ru": "Нереализовано",
    "ar": "غير محقق"
  },
  "portfolio.needsMarketData": {
    "de": "Marktdaten erforderlich",
    "ru": "Нужны рыночные данные",
    "ar": "يلزم بيانات السوق"
  },
  "portfolio.walletTitle": {
    "de": "Wallet & verfügbare Mittel",
    "ru": "Кошелёк и доступные средства",
    "ar": "المحفظة والأموال المتاحة"
  },
  "portfolio.noCredential": {
    "de": "Polymarket-Credentials nicht bereitgestellt",
    "ru": "Polymarket credentials не provisioned",
    "ar": "Polymarket credentials غير مُجهّزة"
  },
  "portfolio.gotoCredentials": {
    "de": "Provision →",
    "ru": "Provision →",
    "ar": "Provision →"
  },
  "portfolio.depositWallet": {
    "de": "Assets · Deposit Wallet",
    "ru": "Активы · Deposit Wallet",
    "ar": "الأصول · Deposit Wallet"
  },
  "portfolio.depositHint": {
    "de": "Wo pUSD gehalten wird (verfügbare Mittel)",
    "ru": "Где хранится pUSD (доступные средства)",
    "ar": "حيث يُحفظ pUSD (الأموال المتاحة)"
  },
  "portfolio.ownerEoa": {
    "de": "Trading · Owner EOA",
    "ru": "Торговля · Owner EOA",
    "ar": "التداول · Owner EOA"
  },
  "portfolio.ownerHint": {
    "de": "Plattform KMS signiert; gasless, kein Gas gespeichert",
    "ru": "Платформа KMS подписывает; gasless, gas не хранится",
    "ar": "المنصة KMS توقّع؛ gasless، لا gas مخزّن"
  },
  "portfolio.balanceUnknown": {
    "de": "Guthaben nicht verfügbar",
    "ru": "Баланс недоступен",
    "ar": "الرصيد غير متاح"
  },
  "portfolio.cashBalance": {
    "de": "Verfügbares pUSD (live)",
    "ru": "Доступный pUSD (live)",
    "ar": "pUSD المتاح (live)"
  },
  "portfolio.equityTitle": {
    "de": "Equity-Kurve",
    "ru": "Кривая equity",
    "ar": "منحنى equity"
  },
  "portfolio.equityEmpty": {
    "de": "Keine Equity-Kurvendaten",
    "ru": "Нет данных equity curve",
    "ar": "لا بيانات منحنى equity"
  },
  "portfolio.perFollow": {
    "de": "P&L nach Follow",
    "ru": "P&L по подписке",
    "ar": "P&L حسب المتابعة"
  },
  "portfolio.perVenue": {
    "de": "P&L nach Venue",
    "ru": "P&L по venue",
    "ar": "P&L حسب Venue"
  },
  "portfolio.breakdownEmpty": {
    "de": "Keine",
    "ru": "Нет",
    "ar": "لا يوجد"
  },
  "portfolio.positionsTitle": {
    "de": "Offene Positionen",
    "ru": "Открытые позиции",
    "ar": "المراكز المفتوحة"
  },
  "portfolio.positionsEmpty": {
    "de": "Keine offenen Positionen",
    "ru": "Нет открытых позиций",
    "ar": "لا مراكز مفتوحة"
  },
  "portfolio.colMarket": {
    "de": "Markt",
    "ru": "Рынок",
    "ar": "السوق"
  },
  "portfolio.colSize": {
    "de": "Größe",
    "ru": "Размер",
    "ar": "الحجم"
  },
  "portfolio.colAvgCost": {
    "de": "Ø-Kosten",
    "ru": "Сред. цена",
    "ar": "متوسط التكلفة"
  },
  "portfolio.colCostBasis": {
    "de": "Kostenbasis",
    "ru": "База стоимости",
    "ar": "أساس التكلفة"
  },
  "portfolio.colOpenedAt": {
    "de": "Eröffnet",
    "ru": "Открыто",
    "ar": "تاريخ الفتح"
  },
  "portfolio.latencyTitle": {
    "de": "Latenz (Signal → Fill)",
    "ru": "Задержка (сигнал → fill)",
    "ar": "الزمن (إشارة → تنفيذ)"
  },
  "portfolio.latencySummary": {
    "de": "Median {median}s · P95 {p95}s · Block0-Treffer {block0HitRate}",
    "ru": "Медиана {median}s · P95 {p95}s · Block0 hit {block0HitRate}",
    "ar": "الوسيط {median}ث · P95 {p95}ث · Block0 {block0HitRate}"
  },
  "portfolio.block0Disabled": {
    "de": "Aus",
    "ru": "Выкл.",
    "ar": "إيقاف"
  },
  "portfolio.execTitle": {
    "de": "Letzte Fills",
    "ru": "Последние fills",
    "ar": "آخر عمليات التنفيذ"
  },
  "portfolio.execEmpty": {
    "de": "Noch keine Fills",
    "ru": "Пока нет fills",
    "ar": "لا عمليات تنفيذ"
  },
  "portfolio.colTime": {
    "de": "Zeit",
    "ru": "Время",
    "ar": "الوقت"
  },
  "portfolio.colSide": {
    "de": "Seite",
    "ru": "Сторона",
    "ar": "الاتجاه"
  },
  "portfolio.colPrice": {
    "de": "Preis",
    "ru": "Цена",
    "ar": "السعر"
  },
  "portfolio.colFee": {
    "de": "Gebühr",
    "ru": "Комиссия",
    "ar": "الرسوم"
  },
  "portfolio.loadError": {
    "de": "Laden fehlgeschlagen: {message}",
    "ru": "Ошибка загрузки: {message}",
    "ar": "فشل التحميل: {message}"
  },
  "portfolio.exportSuccess": {
    "de": "CSV exportiert",
    "ru": "CSV экспортирован",
    "ar": "تم تصدير CSV"
  },
  "portfolio.exportError": {
    "de": "Export fehlgeschlagen: {message}",
    "ru": "Ошибка экспорта: {message}",
    "ar": "فشل التصدير: {message}"
  },
  "trader.loadErrorTitle": {
    "de": "Laden fehlgeschlagen",
    "ru": "Ошибка загрузки",
    "ar": "فشل التحميل"
  },
  "trader.tagHot": {
    "de": "Hot",
    "ru": "Hot",
    "ar": "Hot"
  },
  "trader.officialProfile": {
    "de": "Offizielles Profil ↗",
    "ru": "Официальный профиль ↗",
    "ar": "الملف الرسمي ↗"
  },
  "trader.officialTitle": {
    "de": "Diesen Trader auf der offiziellen Seite ansehen",
    "ru": "Посмотреть трейдера на официальном сайте",
    "ar": "عرض هذا المتداول على الموقع الرسمي"
  },
  "trader.follow": {
    "de": "Diesem Trader folgen",
    "ru": "Подписаться на трейдера",
    "ar": "متابعة هذا المتداول"
  },
  "trader.watch": {
    "de": "👁 Beobachten",
    "ru": "👁 Наблюдать",
    "ar": "👁 مراقبة"
  },
  "trader.watchAdded": {
    "de": "Zur Watchlist hinzugefügt",
    "ru": "Добавлено в watchlist",
    "ar": "أُضيف لقائمة المراقبة"
  },
  "trader.watchExists": {
    "de": "Bereits auf Watchlist",
    "ru": "Уже в watchlist",
    "ar": "موجود في قائمة المراقبة"
  },
  "trader.connectToFollow": {
    "de": "Wallet verbinden zum Folgen",
    "ru": "Подключите кошелёк для подписки",
    "ar": "اربط المحفظة للمتابعة"
  },
  "trader.equityTitle": {
    "de": "Equity-Kurve",
    "ru": "Кривая equity",
    "ar": "منحنى equity"
  },
  "trader.kpiEmpty": {
    "de": "Keine Performance für Zeitraum ({period})",
    "ru": "Нет performance за период ({period})",
    "ar": "لا أداء للفترة ({period})"
  },
  "trader.officialPnlSubDelta": {
    "de": "Wert-Delta approx · inkl. Ein-/Auszahlungen",
    "ru": "Value-delta approx · вкл. депозиты/выводы",
    "ar": "تقريب delta القيمة · يشمل الإيداع/السحب"
  },
  "trader.officialPnlSubLb": {
    "de": "Polymarket-Ranglisten-Definition",
    "ru": "Определение Polymarket leaderboard",
    "ar": "تعريف Polymarket leaderboard"
  },
  "trader.officialPnl": {
    "de": "PnL (official)",
    "ru": "PnL (official)",
    "ar": "PnL (رسمي)"
  },
  "trader.realizedNoOfficial": {
    "de": "Berechnet · keine offiziellen Daten",
    "ru": "Вычислено · нет official данных",
    "ar": "محسوب · لا بيانات رسمية"
  },
  "trader.realizedPnl": {
    "de": "Realisierter PnL",
    "ru": "Реализованный PnL",
    "ar": "PnL محقق"
  },
  "trader.winRate": {
    "de": "Gewinnrate",
    "ru": "Винрейт",
    "ar": "نسبة الفوز"
  },
  "trader.maxDrawdown": {
    "de": "Max. Drawdown",
    "ru": "Макс. просадка",
    "ar": "أقصى انخفاض"
  },
  "trader.realizedSelf": {
    "de": "Realisierter PnL (berechnet)",
    "ru": "Реализованный PnL (вычислено)",
    "ar": "PnL محقق (محسوب)"
  },
  "trader.realizedSelfSub": {
    "de": "Sharpside local replay",
    "ru": "Sharpside local replay",
    "ar": "Sharpside local replay"
  },
  "trader.totalVolume": {
    "de": "Volumen",
    "ru": "Объём",
    "ar": "الحجم"
  },
  "trader.openPositions": {
    "de": "Offene Positionen",
    "ru": "Открытые позиции",
    "ar": "المراكز المفتوحة"
  },
  "trader.positionCount": {
    "de": "Trades",
    "ru": "Сделки",
    "ar": "الصفقات"
  },
  "trader.equityEmpty": {
    "de": "Keine Equity-Kurvendaten",
    "ru": "Нет данных equity curve",
    "ar": "لا بيانات منحنى equity"
  },
  "trader.equityEmptyPeriod": {
    "de": "Keine Kurvendaten für Zeitraum ({period})",
    "ru": "Нет curve данных за период ({period})",
    "ar": "لا بيانات منحنى للفترة ({period})"
  },
  "trader.dataPoints": {
    "de": "{count} Punkte · {start} → {end}",
    "ru": "{count} точек · {start} → {end}",
    "ar": "{count} نقطة · {start} → {end}"
  },
  "trader.positionsTitle": {
    "de": "Offene Positionen",
    "ru": "Открытые позиции",
    "ar": "المراكز المفتوحة"
  },
  "trader.positionsEmpty": {
    "de": "Keine offenen Positionen",
    "ru": "Нет открытых позиций",
    "ar": "لا مراكز مفتوحة"
  },
  "trader.colSize": {
    "de": "Größe",
    "ru": "Размер",
    "ar": "الحجم"
  },
  "trader.colAvgCost": {
    "de": "Ø-Kosten",
    "ru": "Сред. цена",
    "ar": "متوسط التكلفة"
  },
  "trader.colOpenedAt": {
    "de": "Eröffnet",
    "ru": "Открыто",
    "ar": "تاريخ الفتح"
  },
  "trader.positionsError": {
    "de": "Positionen konnten nicht geladen werden: {message}",
    "ru": "Ошибка загрузки позиций: {message}",
    "ar": "فشل تحميل المراكز: {message}"
  },
  "trader.tradesTitle": {
    "de": "Letzte Trades",
    "ru": "Последние сделки",
    "ar": "آخر الصفقات"
  },
  "trader.tradesEmpty": {
    "de": "Noch keine Trades",
    "ru": "Пока нет сделок",
    "ar": "لا صفقات"
  },
  "trader.colTime": {
    "de": "Zeit",
    "ru": "Время",
    "ar": "الوقت"
  },
  "trader.colSide": {
    "de": "Seite",
    "ru": "Сторона",
    "ar": "الاتجاه"
  },
  "trader.colQty": {
    "de": "Größe",
    "ru": "Размер",
    "ar": "الحجم"
  },
  "trader.colPrice": {
    "de": "Preis",
    "ru": "Цена",
    "ar": "السعر"
  },
  "trader.tradesError": {
    "de": "Trades konnten nicht geladen werden: {message}",
    "ru": "Ошибка загрузки сделок: {message}",
    "ar": "فشل تحميل الصفقات: {message}"
  },
  "trader.botTitle": {
    "de": "Bot-Erkennung",
    "ru": "Обнаружение bot",
    "ar": "كشف البots"
  },
  "trader.botNormal": {
    "de": "Normal",
    "ru": "Нормально",
    "ar": "طبيعي"
  },
  "trader.botConfidence": {
    "de": "Konfidenz {percent}%",
    "ru": "Уверенность {percent}%",
    "ar": "ثقة {percent}%"
  },
  "trader.botRulesHit": {
    "de": "· {count} Regeln ausgelöst",
    "ru": "· {count} правил сработало",
    "ar": "· {count} قواعد مُفعّلة"
  },
  "trader.botWarning": {
    "de": "Von botfilter als Bot/Market-Maker markiert. Folgen kann Churn- oder Hedge-Strategien kopieren — Vorsicht.",
    "ru": "Помечен botfilter как bot/market-maker. Подписка может копировать churn или hedge — осторожно.",
    "ar": "مُعلَّم botfilter كبوت/market-maker. المتابعة قد تنسخ churn أو hedge — كن حذراً."
  },
  "trader.botUnknownRule": {
    "de": "Unbekannte Regel",
    "ru": "Неизвестное правило",
    "ar": "قاعدة غير معروفة"
  },
  "trader.botNoRules": {
    "de": "Keine botfilter-Regeln ausgelöst.",
    "ru": "Ни одно правило botfilter не сработало.",
    "ar": "لم تُفعَّل قواعد botfilter."
  },
  "trader.ruleHighFreq": {
    "de": "Hochfrequente Symmetrie (MM/Scalper)",
    "ru": "Высокочастотная симметрия (MM/scalper)",
    "ar": "تماثل عالي التردد (MM/scalper)"
  },
  "trader.ruleWash": {
    "de": "Gleiche-Tx-Hedge-Beine (Wash Trade)",
    "ru": "Hedge legs в одной tx (wash trade)",
    "ar": "أرجل hedge في tx واحدة (wash trade)"
  },
  "trader.ruleRoundTrip": {
    "de": "Kurzes Round-Trip-Fenster + kurze Haltedauer",
    "ru": "Короткое round-trip окно + короткое удержание",
    "ar": "round-trip قصير + احتفاظ قصير"
  },
  "trader.ruleTakerOnly": {
    "de": "Viele Round-Trips + niedrige Gewinnrate (No-Edge-Churner)",
    "ru": "Много round-trips + низкий винрейт (no-edge churner)",
    "ar": "round-trips كثيرة + win rate منخفض (no-edge churner)"
  },
  "trader.ruleSizeConc": {
    "de": "Größe konzentriert in wenigen Märkten (Pump/Single-Market-MM)",
    "ru": "Размер сосредоточен в few markets (pump/single-market MM)",
    "ar": "حجم مركّز في أسواق قليلة (pump/single-market MM)"
  },
  "trader.ruleHighChurn": {
    "de": "Hoher Churn + niedrige Gewinnrate (Noise-Bot)",
    "ru": "Высокий churn + низкий винрейт (noise bot)",
    "ar": "churn عالٍ + win rate منخفض (noise bot)"
  },
  "subscription.title": {
    "de": "Abo",
    "ru": "Подписка",
    "ar": "الاشتراك"
  },
  "subscription.tierComparison": {
    "de": "Stufenvergleich",
    "ru": "Сравнение тарифов",
    "ar": "مقارنة المستويات"
  },
  "subscription.currentTierLabel": {
    "de": "Aktuelle Stufe:",
    "ru": "Текущий тариф:",
    "ar": "المستوى الحالي:"
  },
  "subscription.proActiveBadge": {
    "de": "Pro+ aktiv",
    "ru": "Pro+ активен",
    "ar": "Pro+ نشط"
  },
  "subscription.subscribedUntil": {
    "de": "Abo bis {date}",
    "ru": "Подписка до {date}",
    "ar": "اشتراك حتى {date}"
  },
  "subscription.upgradePitch": {
    "de": "Upgrade auf Pro+ für Kanal B, Cross-Venue-Ausführung, erweitertes Risiko und unbegrenzte Follow-Slots.",
    "ru": "Upgrade Pro+ для Канала B, cross-venue, расширенного риска и безлимит слотов.",
    "ar": "ترقية Pro+ لفتح القناة B وcross-venue ومخاطر متقدمة وslots غير محدود."
  },
  "subscription.featChannelA": {
    "de": "Kanal A · TG Deposit Wallet delegierte Signatur",
    "ru": "Канал A · делегированная подпись TG Deposit Wallet",
    "ar": "القناة A · توقيع مفوض TG Deposit Wallet"
  },
  "subscription.featChannelBPro": {
    "de": "Kanal B · self-hosted daemon (Pro+)",
    "ru": "Канал B · self-hosted daemon (Pro+)",
    "ar": "القناة B · self-hosted daemon (Pro+)"
  },
  "subscription.featChannelBZero": {
    "de": "Kanal B · self-hosted daemon (zero-key)",
    "ru": "Канал B · self-hosted daemon (zero-key)",
    "ar": "القناة B · self-hosted daemon (zero-key)"
  },
  "subscription.featSingleVenue": {
    "de": "Single-Venue-Ausführung",
    "ru": "Исполнение single-venue",
    "ar": "تنفيذ Venue واحد"
  },
  "subscription.featCrossVenuePro": {
    "de": "Cross-Venue-Ausführung (Pro+)",
    "ru": "Cross-venue исполнение (Pro+)",
    "ar": "تنفيذ Cross-Venue (Pro+)"
  },
  "subscription.featSingleCross": {
    "de": "Single- + Cross-Venue-Ausführung",
    "ru": "Single + cross-venue исполнение",
    "ar": "تنفيذ Venue واحد + Cross-Venue"
  },
  "subscription.featBasicRisk": {
    "de": "Basis-Risiko (pro Order / täglich / offene Caps)",
    "ru": "Базовый риск (per-order / daily / open caps)",
    "ar": "مخاطر أساسية (per-order / daily / open caps)"
  },
  "subscription.featAdvancedRiskBasic": {
    "de": "Erweitertes Risiko (Verlustserie / rapid-flip usw.)",
    "ru": "Расширенный риск (loss streak / rapid-flip и т.д.)",
    "ar": "مخاطر متقدمة (loss streak / rapid-flip إلخ)"
  },
  "subscription.featAdvancedRiskFull": {
    "de": "Erweitertes Risiko (Verlustserie / rapid-flip / Slippage / min notional)",
    "ru": "Расширенный риск (loss streak / rapid-flip / slippage / min notional)",
    "ar": "مخاطر متقدمة (loss streak / rapid-flip / slippage / min notional)"
  },
  "subscription.featSlotsLimited": {
    "de": "{count} Follow-Slots",
    "ru": "{count} слотов подписок",
    "ar": "{count} slots متابعة"
  },
  "subscription.featSlotsUnlimited": {
    "de": "Unbegrenzte Follow-Slots",
    "ru": "Безлимит слотов подписок",
    "ar": "slots متابعة غير محدود"
  },
  "subscription.currentTierBadge": {
    "de": "Aktuelle Stufe",
    "ru": "Текущий тариф",
    "ar": "المستوى الحالي"
  },
  "subscription.renew": {
    "de": "Verlängern",
    "ru": "Продлить",
    "ar": "تجديد"
  },
  "subscription.renewModalTitle": {
    "de": "Pro+ verlängern",
    "ru": "Продление Pro+",
    "ar": "تجديد Pro+"
  },
  "subscription.upgrade": {
    "de": "Upgrade Pro+",
    "ru": "Upgrade Pro+",
    "ar": "ترقية Pro+"
  },
  "subscription.usageTitle": {
    "de": "Nutzung",
    "ru": "Использование",
    "ar": "الاستخدام"
  },
  "subscription.followSlotsLabel": {
    "de": "Follow-Slots:",
    "ru": "Слоты подписок:",
    "ar": "slots المتابعة:"
  },
  "subscription.followSlotsCount": {
    "de": "{used} / ∞",
    "ru": "{used} / ∞",
    "ar": "{used} / ∞"
  },
  "subscription.channelBNote": {
    "de": "Kanal B (daemon) und Cross-Venue werden mit Credentials/Konfiguration freigeschaltet.",
    "ru": "Канал B (daemon) и cross-venue открываются с credentials/конфигом.",
    "ar": "القناة B (daemon) وcross-venue تُفعَّل مع credentials/الإعداد."
  },
  "subscription.cancelSubscription": {
    "de": "Abo kündigen",
    "ru": "Отменить подписку",
    "ar": "إلغاء الاشتراك"
  },
  "subscription.cancelConfirm": {
    "de": "Pro+ kündigen und auf Free downgraden?",
    "ru": "Отменить Pro+ и перейти на Free?",
    "ar": "إلغاء Pro+ والعودة إلى Free?"
  },
  "subscription.cancelSuccess": {
    "de": "Abo gekündigt",
    "ru": "Подписка отменена",
    "ar": "تم إلغاء الاشتراك"
  },
  "subscription.paymentComingSoon": {
    "de": "Zahlungen demnächst. Im Test können Sie Pro+ direkt aktivieren/verlängern (1 Monat).",
    "ru": "Платежи скоро. В тесте можно активировать/продлить Pro+ (1 месяц).",
    "ar": "المدفوعات قريباً. في الاختبار يمكن تفعيل/تجديد Pro+ (شهر)."
  },
  "subscription.paymentReplaceNote": {
    "de": "Dieser Eintrag wird zur Zahlungsseite, wenn Billing live ist.",
    "ru": "Эта запись станет страницей оплаты при запуске billing.",
    "ar": "سيصبح هذا مدخل صفحة الدفع عند إطلاق billing."
  },
  "subscription.activateTest": {
    "de": "Aktivieren (Test)",
    "ru": "Активировать (test)",
    "ar": "تفعيل (test)"
  },
  "subscription.activateSuccess": {
    "de": "Pro+ aktiviert (Test)",
    "ru": "Pro+ активирован (test)",
    "ar": "تم تفعيل Pro+ (test)"
  },
  "credentials.title": {
    "de": "Venue credentials",
    "ru": "Venue credentials",
    "ar": "اعتمادات Venue"
  },
  "credentials.live": {
    "de": "✅ Bereitgestellt (online)",
    "ru": "✅ Provisioned (online)",
    "ar": "✅ Provisioned (online)"
  },
  "credentials.offline": {
    "de": "⚠ Bereitgestellt (offline)",
    "ru": "⚠ Provisioned (offline)",
    "ar": "⚠ Provisioned (offline)"
  },
  "credentials.configured": {
    "de": "✅ Konfiguriert",
    "ru": "✅ Configured",
    "ar": "✅ Configured"
  },
  "credentials.notProvisioned": {
    "de": "⚠ Nicht bereitgestellt",
    "ru": "⚠ Not provisioned",
    "ar": "⚠ Not provisioned"
  },
  "credentials.stepperTitle": {
    "de": "Provision-Zustandsautomat",
    "ru": "Provision state machine",
    "ar": "Provision state machine"
  },
  "credentials.stepGenerateOwner": {
    "de": "Owner EOA generieren",
    "ru": "Generate owner EOA",
    "ar": "Generate owner EOA"
  },
  "credentials.stepKms": {
    "de": "Private Key KMS-verschlüsseln",
    "ru": "KMS-encrypt private key",
    "ar": "KMS-encrypt private key"
  },
  "credentials.stepCreate2": {
    "de": "CREATE2-Adresse ableiten",
    "ru": "CREATE2 derive address",
    "ar": "CREATE2 derive address"
  },
  "credentials.stepRelayer": {
    "de": "Relayer deployen",
    "ru": "Relayer deploy",
    "ar": "Relayer deploy"
  },
  "credentials.stepL1L2": {
    "de": "L1 → L2 Credentials",
    "ru": "L1 → L2 credentials",
    "ar": "L1 → L2 credentials"
  },
  "credentials.stepBalance": {
    "de": "Guthaben-Sync",
    "ru": "Balance sync",
    "ar": "Balance sync"
  },
  "credentials.stepPersist": {
    "de": "Persistieren",
    "ru": "Persist",
    "ar": "Persist"
  },
  "credentials.kindDelegated": {
    "de": "Kanal A · Deposit Wallet delegierte Signatur",
    "ru": "Канал A · делегированная подпись Deposit Wallet",
    "ar": "القناة A · توقيع مفوض Deposit Wallet"
  },
  "credentials.kindSession": {
    "de": "Session Wallet (Legacy)",
    "ru": "Session Wallet (legacy)",
    "ar": "Session Wallet (legacy)"
  },
  "credentials.kindFallback": {
    "de": "Credential · {kind}",
    "ru": "Credential · {kind}",
    "ar": "Credential · {kind}"
  },
  "credentials.viewDelegation": {
    "de": "Delegation ansehen →",
    "ru": "View delegation →",
    "ar": "View delegation →"
  },
  "credentials.reprovision": {
    "de": "Neu bereitstellen",
    "ru": "Re-provision",
    "ar": "Re-provision"
  },
  "credentials.reprovisionConfirm": {
    "de": "Neu-Provision erstellt neue Owner EOA. Assets manuell von alter Deposit Wallet migrieren. Fortfahren?",
    "ru": "Re-provisioning создаст новый owner EOA. Мигрируйте активы вручную. Продолжить?",
    "ar": "Re-provisioning ينشئ owner EOA جديداً. انقل الأصول يدوياً. متابعة?"
  },
  "credentials.reprovisionSuccess": {
    "de": "Neu bereitgestellt",
    "ru": "Re-provisioned",
    "ar": "Re-provisioned"
  },
  "credentials.revokePhase2": {
    "de": "Widerrufen (Phase 2) 🔒",
    "ru": "Revoke (Phase 2) 🔒",
    "ar": "Revoke (Phase 2) 🔒"
  },
  "credentials.loadError": {
    "de": "Polymarket-Credentials konnten nicht geladen werden: {message}",
    "ru": "Ошибка загрузки Polymarket credentials: {message}",
    "ar": "فشل تحميل Polymarket credentials: {message}"
  },
  "credentials.kalshiDesc": {
    "de": "Kanal A/B · KYC + API key",
    "ru": "Канал A/B · KYC + API key",
    "ar": "القناة A/B · KYC + API key"
  },
  "credentials.manifoldDesc": {
    "de": "Nur Signal · API key",
    "ru": "Signal only · API key",
    "ar": "Signal only · API key"
  },
  "credentials.configureLocked": {
    "de": "Konfigurieren ({phase}) 🔒",
    "ru": "Configure ({phase}) 🔒",
    "ar": "Configure ({phase}) 🔒"
  },
  "credentials.daemonSectionTitle": {
    "de": "Kanal B · daemon API key (Cross-Venue)",
    "ru": "Канал B · daemon API key (cross-venue)",
    "ar": "القناة B · daemon API key (cross-venue)"
  },
  "credentials.daemonSectionDesc": {
    "de": "Von self-hosted daemon zum Abrufen von Copy-Anweisungen. Klartext einmal bei Ausstellung.",
    "ru": "Для self-hosted daemon pull copy instructions. Plaintext один раз при выдаче.",
    "ar": "لـ self-hosted daemon لسحب تعليمات النسخ. Plaintext مرة عند الإصدار."
  },
  "credentials.daemonManage": {
    "de": "daemon-Key verwalten →",
    "ru": "Manage daemon key →",
    "ar": "Manage daemon key →"
  },
  "delegation.title": {
    "de": "Delegation",
    "ru": "Делегирование",
    "ar": "التفويض"
  },
  "delegation.assetRights": {
    "de": "Asset-Rechte",
    "ru": "Права на активы",
    "ar": "حقوق الأصول"
  },
  "delegation.tradingRights": {
    "de": "Trading-Rechte",
    "ru": "Торговые права",
    "ar": "حقوق التداول"
  },
  "delegation.platformKms": {
    "de": "Plattform KMS-Signatur",
    "ru": "Подпись Platform KMS",
    "ar": "توقيع Platform KMS"
  },
  "delegation.exportOwner": {
    "de": "Owner exportieren und selbst rotieren möglich",
    "ru": "Можно экспортировать owner и rotate самостоятельно",
    "ar": "يمكنك تصدير owner والتدوير بنفسك"
  },
  "delegation.assetStorage": {
    "de": "Wo Assets gehalten werden",
    "ru": "Где хранятся активы",
    "ar": "أين تُحفظ الأصول"
  },
  "delegation.platformTransfer": {
    "de": "Plattform kann WALLET-Transfers signieren",
    "ru": "Платформа может подписывать WALLET transfers",
    "ar": "المنصة يمكنها توقيع WALLET transfers"
  },
  "delegation.platformOrders": {
    "de": "Plattform kann Orders für Sie platzieren",
    "ru": "Платформа может размещать ордера",
    "ar": "المنصة يمكنها وضع أوامر نيابةً عنك"
  },
  "delegation.provisionStatus": {
    "de": "Provision-Status",
    "ru": "Статус provision",
    "ar": "حالة Provision"
  },
  "delegation.provisionLive": {
    "de": "Vollständig online bereitgestellt",
    "ru": "Полностью provisioned online",
    "ar": "Provisioned online بالكامل"
  },
  "delegation.provisionOffline": {
    "de": "Offline-Modus: überspringt Relayer deploy / L1 derive / batch approve / balance sync (dry_run OK)",
    "ru": "Offline mode: пропускает Relayer deploy / L1 derive / batch approve / balance sync (dry_run OK)",
    "ar": "Offline mode: يتخطى Relayer deploy / L1 derive / batch approve / balance sync (dry_run OK)"
  },
  "delegation.detailsTitle": {
    "de": "Credential-Details",
    "ru": "Детали credential",
    "ar": "تفاصيل Credential"
  },
  "delegation.labelPlatform": {
    "de": "Plattform",
    "ru": "Платформа",
    "ar": "المنصة"
  },
  "delegation.labelMode": {
    "de": "Modus",
    "ru": "Режим",
    "ar": "الوضع"
  },
  "delegation.modeOnline": {
    "de": "Online",
    "ru": "Online",
    "ar": "Online"
  },
  "delegation.modeOffline": {
    "de": "Offline (dev)",
    "ru": "Offline (dev)",
    "ar": "Offline (dev)"
  },
  "delegation.serverKeyNote": {
    "de": "Hinweis: encrypted_owner_key / encrypted_l2_secret bleiben serverseitig; in der UI nicht sichtbar.",
    "ru": "Примечание: encrypted_owner_key / encrypted_l2_secret на сервере; не видны в UI.",
    "ar": "ملاحظة: encrypted_owner_key / encrypted_l2_secret على الخادم؛ غير مرئية في UI."
  },
  "delegation.reprovision": {
    "de": "Re-provision",
    "ru": "Re-provision",
    "ar": "Re-provision"
  },
  "delegation.reprovisionConfirm": {
    "de": "Neu-Provision erstellt neue Owner EOA. Assets manuell migrieren. Fortfahren?",
    "ru": "Re-provisioning создаст новый owner EOA. Мигрируйте активы вручную. Продолжить?",
    "ar": "Re-provisioning ينشئ owner EOA جديداً. انقل الأصول يدوياً. متابعة?"
  },
  "delegation.reprovisionSuccess": {
    "de": "Re-provisioned",
    "ru": "Re-provisioned",
    "ar": "Re-provisioned"
  },
  "delegation.revokeTitle": {
    "de": "Revoke delegation",
    "ru": "Отозвать delegation",
    "ar": "Revoke delegation"
  },
  "delegation.revoke": {
    "de": "Revoke delegation",
    "ru": "Отозвать delegation",
    "ar": "Revoke delegation"
  },
  "delegation.revokePhase2": {
    "de": "Revoke delegation (Phase 2) 🔒",
    "ru": "Revoke delegation (Phase 2) 🔒",
    "ar": "Revoke delegation (Phase 2) 🔒"
  },
  "delegation.revokePhase2Note": {
    "de": "Self-serve revoke lands in Phase 2; for emergencies contact support.",
    "ru": "Self-serve revoke в Phase 2; для экстренных случаев — support.",
    "ar": "Self-serve revoke في Phase 2؛ للطوارئ contact support."
  },
  "delegation.selfCustodyTitle": {
    "de": "Upgrade to non-custodial (Phase 2)",
    "ru": "Upgrade to non-custodial (Phase 2)",
    "ar": "Upgrade to non-custodial (Phase 2)"
  },
  "delegation.selfCustodyDesc": {
    "de": "Now: delegated trading (platform holds owner key) → Goal: non-custodial (you hold owner key; platform only relays signatures)",
    "ru": "Сейчас: delegated trading (платформа держит owner key) → Цель: non-custodial (вы держите owner key; платформа relay signatures)",
    "ar": "الآن: delegated trading (المنصة تحتفظ owner key) → الهدف: non-custodial (أنت تحتفظ owner key؛ المنصة relay signatures)"
  },
  "delegation.notifyMigration": {
    "de": "Bei Migration benachrichtigen",
    "ru": "Уведомить о миграции",
    "ar": "Notify me for migration"
  },
  "delegation.notifySuccess": {
    "de": "Added to Phase 2 migration notify list",
    "ru": "Добавлено в Phase 2 migration notify list",
    "ar": "Added to Phase 2 migration notify list"
  },
  "delegation.notProvisioned": {
    "de": "Polymarket-Delegation nicht bereitgestellt",
    "ru": "Polymarket delegation не provisioned",
    "ar": "Polymarket delegation not provisioned"
  },
  "delegation.provisionNow": {
    "de": "Provision now",
    "ru": "Provision сейчас",
    "ar": "Provision now"
  },
  "delegation.provisionSuccess": {
    "de": "Bereitgestellt",
    "ru": "Provisioned",
    "ar": "Provisioned"
  },
  "delegation.loadError": {
    "de": "Failed to load: {message}",
    "ru": "Ошибка загрузки: {message}",
    "ar": "فشل التحميل: {message}"
  },
  "delegation.whatIsCustody": {
    "de": "  ⓘ What is custody level?",
    "ru": "  ⓘ Что такое custody level?",
    "ar": "  ⓘ What is custody level?"
  },
  "delegation.channelADesc": {
    "de": "Kanal A: Deposit Wallet (ERC-1967) hält Assets + Plattform KMS signiert Trades. Phase 2 gibt Trading-Rechte zurück.",
    "ru": "Канал A: Deposit Wallet (ERC-1967) хранит активы + platform KMS подписывает trades. Phase 2 возвращает trading rights.",
    "ar": "Channel A: Deposit Wallet (ERC-1967) holds assets + platform KMS signs trades. Phase 2 returns trading rights to you."
  },
  "daemonKey.title": {
    "de": "daemon API key (Channel B)",
    "ru": "daemon API key (Channel B)",
    "ar": "daemon API key (Channel B)"
  },
  "daemonKey.statusLabel": {
    "de": "Status:",
    "ru": "Статус:",
    "ar": "Status:"
  },
  "daemonKey.statusIssued": {
    "de": "✅ Ausgestellt",
    "ru": "✅ Выдан",
    "ar": "✅ Issued"
  },
  "daemonKey.statusNotIssued": {
    "de": "❌ Nicht ausgestellt",
    "ru": "❌ Не выдан",
    "ar": "❌ Not issued"
  },
  "daemonKey.lastRotated": {
    "de": "Last rotated: {datetime}",
    "ru": "Последняя ротация: {datetime}",
    "ar": "Last rotated: {datetime}"
  },
  "daemonKey.description": {
    "de": "Für Kanal B (self-hosted daemon) zum Abrufen von Copy-Anweisungen. Plattform speichert nur Hash; Klartext einmal bei Ausstellung.",
    "ru": "Для Канала B (self-hosted daemon) pull copy instructions. Платформа хранит только hash; plaintext один раз.",
    "ar": "Used by Channel B (self-hosted daemon) to pull copy instructions. Platform stores hash only; plaintext shown once at issue."
  },
  "daemonKey.rotate": {
    "de": "Rotate key",
    "ru": "Rotate key",
    "ar": "Rotate key"
  },
  "daemonKey.issue": {
    "de": "Issue key",
    "ru": "Issue key",
    "ar": "Issue key"
  },
  "daemonKey.rotateConfirm": {
    "de": "Rotation invalidiert den alten Key sofort. Alle daemon-Konfigurationen aktualisieren. Fortfahren?",
    "ru": "Ротация аннулирует старый key. Обновите все daemon configs. Продолжить?",
    "ar": "Rotating invalidates the old key immediately. Update all daemon configs. Continue?"
  },
  "daemonKey.issueConfirm": {
    "de": "daemon API key ausstellen?",
    "ru": "Выдать daemon API key?",
    "ar": "Issue a daemon API key?"
  },
  "daemonKey.oneTimeTitle": {
    "de": "daemon API key (shown once)",
    "ru": "daemon API key (shown once)",
    "ar": "daemon API key (shown once)"
  },
  "daemonKey.oneTimeWarn": {
    "de": "Jetzt speichern — danach nicht mehr sichtbar. Rotation invalidiert den alten Key sofort.",
    "ru": "Сохраните сейчас — больше не покажем. Ротация аннулирует старый key.",
    "ar": "Save it now — you won't see it again. Rotation invalidates the old key immediately."
  },
  "daemonKey.installTitle": {
    "de": "Daemon install",
    "ru": "Установка daemon",
    "ar": "Daemon install"
  },
  "daemonKey.stepDownload": {
    "de": "Daemon-Binary für Ihre Plattform herunterladen",
    "ru": "Скачать daemon binary для вашей платформы",
    "ar": "Download daemon binary for your platform"
  },
  "daemonKey.downloadPending": {
    "de": "(build artifact pending)",
    "ru": "(build artifact pending)",
    "ar": "(build artifact pending)"
  },
  "daemonKey.stepConfigure": {
    "de": "Configure .env",
    "ru": "Configure .env",
    "ar": "Configure .env"
  },
  "daemonKey.stepRun": {
    "de": "Run daemon",
    "ru": "Run daemon",
    "ar": "Run daemon"
  },
  "daemonKey.docsLink": {
    "de": "Full docs → (coming soon)",
    "ru": "Полная документация → (скоро)",
    "ar": "Full docs → (coming soon)"
  },
  "watchlist.title": {
    "de": "Watchlist",
    "ru": "Список наблюдения",
    "ar": "قائمة المراقبة"
  },
  "watchlist.titleCount": {
    "de": "Watchlist ({count})",
    "ru": "Список наблюдения ({count})",
    "ar": "قائمة المراقبة ({count})"
  },
  "watchlist.discover": {
    "de": "+ Trader entdecken",
    "ru": "+ Найти трейдеров",
    "ar": "+ اكتشف المتداولين"
  },
  "watchlist.loadError": {
    "de": "Failed to load: {message}",
    "ru": "Ошибка загрузки: {message}",
    "ar": "فشل التحميل: {message}"
  },
  "watchlist.addSuccess": {
    "de": "Zur Watchlist hinzugefügt",
    "ru": "Добавлено в watchlist",
    "ar": "أُضيف لقائمة المراقبة"
  },
  "watchlist.addFailed": {
    "de": "Failed to add to watchlist",
    "ru": "Не удалось добавить в watchlist",
    "ar": "فشل الإضافة لقائمة المراقبة"
  },
  "watchlist.empty": {
    "de": "No watched traders yet",
    "ru": "Пока нет наблюдаемых трейдеров",
    "ar": "لا متداولين مراقَبين"
  },
  "watchlist.emptyAction": {
    "de": "Trader entdecken →",
    "ru": "Найти трейдеров →",
    "ar": "اكتشف المتداولين →"
  },
  "watchlist.identityPrefix": {
    "de": "Identity {id}…",
    "ru": "Идентичность {id}…",
    "ar": "هوية {id}…"
  },
  "watchlist.watchingChip": {
    "de": "👁 Watching",
    "ru": "👁 Наблюдаем",
    "ar": "👁 مراقَب"
  },
  "watchlist.address": {
    "de": "Adresse",
    "ru": "Адрес",
    "ar": "العنوان"
  },
  "watchlist.savedAt": {
    "de": "Gespeichert",
    "ru": "Сохранено",
    "ar": "تاريخ الحفظ"
  },
  "watchlist.perfLoading": {
    "de": "Loading performance…",
    "ru": "Загрузка performance…",
    "ar": "جاري تحميل الأداء…"
  },
  "watchlist.noPerf": {
    "de": "No performance yet",
    "ru": "Пока нет performance",
    "ar": "لا أداء بعد"
  },
  "watchlist.winRate": {
    "de": "Gewinnrate",
    "ru": "Винрейт",
    "ar": "نسبة الفوز"
  },
  "watchlist.realized": {
    "de": "Realized",
    "ru": "Реализовано",
    "ar": "محقق"
  },
  "watchlist.perfUnavailable": {
    "de": "Performance unavailable",
    "ru": "Performance недоступен",
    "ar": "الأداء غير متاح"
  },
  "watchlist.identityPerfPending": {
    "de": "Cross-venue identity performance (pending endpoint)",
    "ru": "Cross-venue identity performance (endpoint в разработке)",
    "ar": "أداء cross-venue identity (endpoint قيد الانتظار)"
  },
  "watchlist.upgradeToFollow": {
    "de": "⇧ Upgrade to follow",
    "ru": "⇧ Upgrade до follow",
    "ar": "⇧ ترقية للمتابعة"
  },
  "watchlist.upgradeSuccess": {
    "de": "Auf Follow upgegradet; Watch-Eintrag verbraucht",
    "ru": "Upgrade до follow; watch item использован",
    "ar": "تمت الترقية للمتابعة؛ استُهلك عنصر المراقبة"
  },
  "watchlist.remove": {
    "de": "🗑 Unwatch",
    "ru": "🗑 Снять с наблюдения",
    "ar": "🗑 إلغاء المراقبة"
  },
  "watchlist.removeConfirm": {
    "de": "Remove this watch item?",
    "ru": "Удалить этот watch item?",
    "ar": "إزالة عنصر المراقبة؟"
  },
  "watchlist.removeSuccess": {
    "de": "Von Watchlist entfernt",
    "ru": "Удалено из watchlist",
    "ar": "أُزيل من قائمة المراقبة"
  },
  "copyHistory.title": {
    "de": "Trade history",
    "ru": "История сделок",
    "ar": "سجل الصفقات"
  },
  "copyHistory.time": {
    "de": "Zeit",
    "ru": "Время",
    "ar": "الوقت"
  },
  "copyHistory.follow": {
    "de": "Follow",
    "ru": "Follow",
    "ar": "Follow"
  },
  "copyHistory.status": {
    "de": "Status",
    "ru": "Статус",
    "ar": "الحالة"
  },
  "copyHistory.last1d": {
    "de": "Last 1 day",
    "ru": "Последний день",
    "ar": "آخر يوم"
  },
  "copyHistory.last1w": {
    "de": "Last 1 week",
    "ru": "Последняя неделя",
    "ar": "آخر أسبوع"
  },
  "copyHistory.last1m": {
    "de": "Last 1 month",
    "ru": "Последний месяц",
    "ar": "آخر شهر"
  },
  "copyHistory.last1y": {
    "de": "Last 1 year",
    "ru": "Последний год",
    "ar": "آخر سنة"
  },
  "copyHistory.loadError": {
    "de": "Failed to load trade history: {message}",
    "ru": "Ошибка загрузки истории сделок: {message}",
    "ar": "فشل تحميل سجل الصفقات: {message}"
  },
  "copyHistory.empty": {
    "de": "No trades",
    "ru": "Нет сделок",
    "ar": "لا صفقات"
  },
  "copyHistory.colMarket": {
    "de": "Markt",
    "ru": "Рынок",
    "ar": "السوق"
  },
  "copyHistory.colSide": {
    "de": "Seite",
    "ru": "Сторона",
    "ar": "الاتجاه"
  },
  "copyHistory.colPrice": {
    "de": "Preis",
    "ru": "Цена",
    "ar": "السعر"
  },
  "copyHistory.colSize": {
    "de": "Größe",
    "ru": "Размер",
    "ar": "الحجم"
  },
  "copyHistory.openPosition": {
    "de": "Open",
    "ru": "Открыта",
    "ar": "مفتوح"
  },
  "copyHistory.noRecords": {
    "de": "No records",
    "ru": "Нет записей",
    "ar": "لا سجلات"
  },
  "copyHistory.showing": {
    "de": "Showing {from}-{to}{more}",
    "ru": "Показано {from}-{to}{more}",
    "ar": "عرض {from}-{to}{more}"
  },
  "copyHistory.exportSuccess": {
    "de": "Exported",
    "ru": "Экспортировано",
    "ar": "تم التصدير"
  },
  "copyHistory.exportFailed": {
    "de": "Export failed: {message}",
    "ru": "Ошибка экспорта: {message}",
    "ar": "فشل التصدير: {message}"
  },
  "importPage.title": {
    "de": "Trader importieren",
    "ru": "Импорт трейдера",
    "ar": "استيراد متداول"
  },
  "importPage.description": {
    "de": "Enter a wallet address to backfill trades and add to watchlist.",
    "ru": "Введите адрес кошелька для backfill сделок и добавления в watchlist.",
    "ar": "أدخل عنوان المحفظة لملء الصفقات وإضافتها لقائمة المراقبة."
  },
  "importPage.platform": {
    "de": "Platform",
    "ru": "Платформа",
    "ar": "المنصة"
  },
  "importPage.walletAddress": {
    "de": "Wallet-Adresse",
    "ru": "Адрес кошелька",
    "ar": "عنوان المحفظة"
  },
  "importPage.submit": {
    "de": "Import & backfill",
    "ru": "Импорт и backfill",
    "ar": "استيراد وملء"
  },
  "importPage.submitting": {
    "de": "Backfill…",
    "ru": "Backfill…",
    "ar": "جاري الملء…"
  },
  "importPage.selectPlatform": {
    "de": "Select a platform",
    "ru": "Выберите платформу",
    "ar": "اختر المنصة"
  },
  "importPage.enterAddress": {
    "de": "Enter a wallet address",
    "ru": "Введите адрес кошелька",
    "ar": "أدخل عنوان المحفظة"
  },
  "importPage.invalidAddress": {
    "de": "Invalid address — expect 0x + 40 hex chars",
    "ru": "Неверный адрес — ожидается 0x + 40 hex",
    "ar": "عنوان غير صالح — 0x + 40 hex"
  },
  "importPage.importFailed": {
    "de": "Import failed",
    "ru": "Импорт не удался",
    "ar": "فشل الاستيراد"
  },
  "importPage.success": {
    "de": "Imported — backfilled {count} trades",
    "ru": "Импортировано — backfill {count} сделок",
    "ar": "تم الاستيراد — ملء {count} صفقة"
  },
  "followForm.title": {
    "de": "Edit follow",
    "ru": "Редактировать подписку",
    "ar": "تعديل المتابعة"
  },
  "followForm.channel": {
    "de": "Kanal",
    "ru": "Канал",
    "ar": "القناة"
  },
  "followForm.executeVenue": {
    "de": "Execute venue",
    "ru": "Venue исполнения",
    "ar": "Venue التنفيذ"
  },
  "followForm.sizingValue": {
    "de": "Sizing value",
    "ru": "Значение sizing",
    "ar": "قيمة sizing"
  },
  "followForm.maxOrder": {
    "de": "Max order notional (USDC, 0=unlimited)",
    "ru": "Макс. notional ордера (USDC, 0=без лимита)",
    "ar": "الحد الأقصى notional (USDC، 0=غير محدود)"
  },
  "followForm.dailyMax": {
    "de": "Daily max (USDC, 0=unlimited)",
    "ru": "Дневной лимит (USDC, 0=без лимита)",
    "ar": "الحد اليومي (USDC، 0=غير محدود)"
  },
  "followForm.maxOpen": {
    "de": "Max open positions (0=unlimited)",
    "ru": "Макс. открытых позиций (0=без лимита)",
    "ar": "الحد الأقصى للمراكز (0=غير محدود)"
  },
  "followForm.channelTg": {
    "de": "TG Deposit Wallet (platform signs)",
    "ru": "TG Deposit Wallet (платформа подписывает)",
    "ar": "TG Deposit Wallet (المنصة توقّع)"
  },
  "followForm.channelDaemon": {
    "de": "Self-hosted daemon (Pro+)",
    "ru": "Self-hosted daemon (Pro+)",
    "ar": "Self-hosted daemon (Pro+)"
  },
  "followForm.sizingFixed": {
    "de": "fixed · fixed USDC per trade",
    "ru": "fixed · фиксированный USDC за сделку",
    "ar": "fixed · USDC ثابت لكل صفقة"
  },
  "followForm.sizingProportional": {
    "de": "proportional · vs leader size",
    "ru": "proportional · vs размер лидера",
    "ar": "proportional · vs حجم القائد"
  },
  "followForm.sizingPercent": {
    "de": "percent_of_balance · % des Guthabens",
    "ru": "percent_of_balance · % баланса",
    "ar": "percent_of_balance · % من الرصيد"
  },
  "followForm.sizingHint": {
    "de": "fixed=USDC; proportional=ratio (0.5=50%); percent_of_balance=% (0.05=5%)",
    "ru": "fixed=USDC; proportional=коэффициент (0.5=50%); percent_of_balance=% (0.05=5%)",
    "ar": "fixed=USDC; proportional=نسبة (0.5=50%)؛ percent_of_balance=% (0.05=5%)"
  },
  "followForm.sameVenueOnly": {
    "de": " same_venue_only (same venue only; off = cross-venue needs Pro+)",
    "ru": " same_venue_only (только тот же venue; выкл = cross-venue нужен Pro+)",
    "ar": " same_venue_only (نفس venue فقط؛ إيقاف = cross-venue يتطلب Pro+)"
  },
  "followForm.errorSizing": {
    "de": "Sizing value must be > 0",
    "ru": "Значение sizing должно быть > 0",
    "ar": "قيمة sizing يجب أن تكون > 0"
  },
  "upgradeForm.title": {
    "de": "Upgrade to follow",
    "ru": "Upgrade до follow",
    "ar": "ترقية للمتابعة"
  },
  "upgradeForm.targetDescription": {
    "de": "Watching: {target} · upgrading starts execution and consumes this watch item",
    "ru": "Наблюдение: {target} · upgrade запускает исполнение и использует watch item",
    "ar": "مراقبة: {target} · الترقية تبدأ التنفيذ وتستهلك عنصر المراقبة"
  },
  "upgradeForm.identityPrefix": {
    "de": "Identity {id}…",
    "ru": "Идентичность {id}…",
    "ar": "هوية {id}…"
  },
  "upgradeForm.advanced": {
    "de": "Advanced risk (optional)",
    "ru": "Расширенный риск (опционально)",
    "ar": "مخاطر متقدمة (اختياري)"
  },
  "upgradeForm.submit": {
    "de": "Upgrade to follow",
    "ru": "Upgrade до follow",
    "ar": "ترقية للمتابعة"
  }
}

def load_js_object(path: Path) -> dict:
    text = path.read_text(encoding="utf-8")
    text = re.sub(r"^export default\s*", "", text.strip())
    text = re.sub(r";\s*$", "", text)
    return json.loads(text)


def flatten(d: dict, prefix: str = "") -> dict[str, str]:
    out: dict[str, str] = {}
    for k, v in d.items():
        key = f"{prefix}.{k}" if prefix else k
        if isinstance(v, dict):
            out.update(flatten(v, key))
        else:
            out[key] = v
    return out


def build_locale_tree(en_node, prefix: str, loc: str) -> dict | str:
    if isinstance(en_node, dict):
        return {
            k: build_locale_tree(v, f"{prefix}.{k}" if prefix else k, loc)
            for k, v in en_node.items()
        }
    return TRANSLATIONS[prefix][loc]


def to_js_module(obj: dict) -> str:
    return f"export default {json.dumps(obj, ensure_ascii=False, indent=2)};\n"


def count_same_as_en(en_flat: dict[str, str], loc_flat: dict[str, str]) -> int:
    return sum(1 for k in en_flat if loc_flat.get(k) == en_flat[k])


_PATCHES_PATH = Path(__file__).resolve().parent / "_translation_patches.json"


def _load_patches() -> dict[str, dict[str, str]]:
    if _PATCHES_PATH.exists():
        return json.loads(_PATCHES_PATH.read_text(encoding="utf-8"))
    return {}


def apply_all_patches() -> None:
    for key, locs in _load_patches().items():
        if key in TRANSLATIONS:
            TRANSLATIONS[key].update(locs)


def main() -> int:
    apply_all_patches()
    en = load_js_object(DIR / "en.js")
    en_flat = flatten(en)
    expected = len(en_flat)

    if len(TRANSLATIONS) != expected:
        print(
            f"TRANSLATIONS count mismatch: en={expected} translations={len(TRANSLATIONS)}",
            file=sys.stderr,
        )
        return 1

    if set(TRANSLATIONS) != set(en_flat):
        print("Key set mismatch", file=sys.stderr)
        return 1

    print(f"en key count: {expected}")
    print(f"TRANSLATIONS key count: {len(TRANSLATIONS)}")

    for loc in LOCALES:
        nested = build_locale_tree(en, "", loc)
        loc_flat = {k: TRANSLATIONS[k][loc] for k in en_flat}
        out_path = DIR / f"{loc}.js"
        out_path.write_text(to_js_module(nested), encoding="utf-8")
        same = count_same_as_en(en_flat, loc_flat)
        print(f"{loc}: keys={len(loc_flat)} same_as_en={same} -> wrote {out_path.name}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
