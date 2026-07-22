# -*- coding: utf-8 -*-
"""Catalog extension data for gen_sharpside_i18n.py — leaf tuples (zh,en,ja,ko,es,fr,de,pt,ru,ar)."""
from __future__ import annotations

LOCALES = ["zh", "en", "ja", "ko", "es", "fr", "de", "pt", "ru", "ar"]


def L(zh, en, ja, ko, es, fr, de, pt, ru, ar):
    return dict(zip(LOCALES, [zh, en, ja, ko, es, fr, de, pt, ru, ar]))


def t(zh, en, ja, ko, es, fr, de, pt, ru, ar):
    return L(zh, en, ja, ko, es, fr, de, pt, ru, ar)


COL = {
    "time": t("时间", "Time", "時間", "시간", "Hora", "Heure", "Zeit", "Hora", "Время", "الوقت"),
    "side": t("方向", "Side", "方向", "방향", "Lado", "Côté", "Seite", "Lado", "Сторона", "الاتجاه"),
    "size": t("数量", "Size", "数量", "수량", "Tamaño", "Taille", "Größe", "Quantidade", "Размер", "الحجم"),
    "price": t("价格", "Price", "価格", "가격", "Precio", "Prix", "Preis", "Preço", "Цена", "السعر"),
    "amount": t("金额", "Amount", "金額", "금액", "Importe", "Montant", "Betrag", "Valor", "Сумма", "المبلغ"),
    "status": t("状态", "Status", "状態", "상태", "Estado", "Statut", "Status", "Estado", "Статус", "الحالة"),
    "market": t("市场", "Market", "マーケット", "시장", "Mercado", "Marché", "Markt", "Mercado", "Рынок", "السوق"),
    "platform": t("平台", "Platform", "プラットフォーム", "플랫폼", "Plataforma", "Plateforme", "Plattform", "Plataforma", "Платформа", "المنصة"),
    "winRate": t("胜率", "Win rate", "勝率", "승률", "Tasa de acierto", "Taux de réussite", "Gewinnrate", "Taxa de acerto", "Винрейт", "نسبة الفوز"),
    "trader": t("交易者", "Trader", "トレーダー", "트레이더", "Trader", "Trader", "Trader", "Trader", "Трейдер", "المتداول"),
    "note": t("备注", "Note", "備考", "비고", "Nota", "Note", "Notiz", "Nota", "Примечание", "ملاحظة"),
    "action": t("操作", "Action", "操作", "작업", "Acción", "Action", "Aktion", "Ação", "Действие", "إجراء"),
    "fee": t("手续费", "Fee", "手数料", "수수료", "Comisión", "Frais", "Gebühr", "Taxa", "Комиссия", "الرسوم"),
    "follow": t("跟随", "Follow", "フォロー", "팔로우", "Seguimiento", "Suivi", "Follow", "Seguir", "Следование", "المتابعة"),
    "txHash": t("交易哈希", "Tx hash", "Tx ハッシュ", "트랜잭션 해시", "Hash de tx", "Hash de tx", "Tx-Hash", "Hash da tx", "Хеш tx", "تجزئة المعاملة"),
    "avgCost": t("均价", "Avg cost", "平均単価", "평균 단가", "Coste medio", "Coût moyen", "Ø-Kosten", "Custo médio", "Сред. цена", "متوسط التكلفة"),
    "toAddress": t("目标地址", "To address", "送金先", "수신 주소", "Dirección destino", "Adresse destinataire", "Zieladresse", "Endereço destino", "Адрес получателя", "عنوان الوجهة"),
    "outcome": t("赢方", "Winning outcome", "勝者", "승리 결과", "Resultado ganador", "Issue gagnante", "Gewinn-Outcome", "Resultado vencedor", "Исход-победитель", "النتيجة الفائزة"),
    "source": t("来源", "Source", "ソース", "출처", "Origen", "Source", "Quelle", "Origem", "Источник", "المصدر"),
    "reason": t("原因", "Reason", "理由", "사유", "Motivo", "Raison", "Grund", "Motivo", "Причина", "السبب"),
    "quantity": t("数量", "Quantity", "数量", "수량", "Cantidad", "Quantité", "Menge", "Quantidade", "Количество", "الكمية"),
    "identity": t("身份", "Identity", "アイデンティティ", "신원", "Identidad", "Identité", "Identität", "Identidade", "Идентичность", "الهوية"),
    "channel": t("通道", "Channel", "チャネル", "채널", "Canal", "Canal", "Kanal", "Canal", "Канал", "القناة"),
    "execute": t("执行", "Execution", "実行", "실행", "Ejecución", "Exécution", "Ausführung", "Execução", "Исполнение", "التنفيذ"),
    "address": t("地址", "Address", "アドレス", "주소", "Dirección", "Adresse", "Adresse", "Endereço", "Адрес", "العنوان"),
    "viewAll": t("全部 →", "View all →", "すべて →", "전체 →", "Ver todo →", "Tout voir →", "Alle →", "Ver tudo →", "Все →", "الكل →"),
    "viewLink": t("查看 →", "View →", "表示 →", "보기 →", "Ver →", "Voir →", "Ansehen →", "Ver →", "Смотреть →", "عرض →"),
    "periodLabel": t("周期", "Period", "期間", "기간", "Periodo", "Période", "Zeitraum", "Período", "Период", "الفترة"),
    "filter": t("筛选", "Filter", "フィルター", "필터", "Filtro", "Filtre", "Filter", "Filtro", "Фильтр", "تصفية"),
    "sort": t("排序", "Sort", "並べ替え", "정렬", "Ordenar", "Tri", "Sortierung", "Ordenar", "Сортировка", "الترتيب"),
    "empty": t("暂无", "None yet", "なし", "없음", "Ninguno", "Aucun", "Keine", "Nenhum", "Нет", "لا يوجد"),
}

PERIOD = {
    k: t(*vals)
    for k, vals in {
        "1d": ("1天", "1D", "1日", "1일", "1D", "1J", "1T", "1D", "1Д", "يوم"),
        "1w": ("1周", "1W", "1週", "1주", "1S", "1S", "1W", "1S", "1Н", "أسبوع"),
        "1m": ("1个月", "1M", "1か月", "1개월", "1M", "1M", "1M", "1M", "1М", "شهر"),
        "1y": ("1年", "1Y", "1年", "1년", "1A", "1A", "1J", "1A", "1Г", "سنة"),
        "ytd": ("年初至今", "YTD", "年初来", "YTD", "YTD", "AAJ", "YTD", "YTD", "С нач. года", "منذ بداية العام"),
        "all": ("全部", "All", "すべて", "전체", "Todo", "Tout", "Alle", "Todos", "Все", "الكل"),
    }.items()
}


def err(zh, en, ja, ko, es, fr, de, pt, ru, ar):
    return t(zh, en, ja, ko, es, fr, de, pt, ru, ar)


def build_nav():
    return {
        "ariaMain": t("主导航", "Main navigation", "メインナビゲーション", "메인 내비게이션", "Navegación principal", "Navigation principale", "Hauptnavigation", "Navegação principal", "Основная навигация", "التنقل الرئيسي"),
        "ariaMobile": t("移动端导航", "Mobile navigation", "モバイルナビゲーション", "모바일 내비게이션", "Navegación móvil", "Navigation mobile", "Mobile Navigation", "Navegação móvel", "Мобильная навигация", "التنقل على الجوال"),
        "discover": t("发现", "Discover", "発見", "발견", "Descubrir", "Découvrir", "Entdecken", "Descobrir", "Обзор", "استكشاف"),
        "copy": t("跟单", "Copy", "コピー", "카피", "Copy", "Copy", "Copy", "Copy", "Copy", "نسخ"),
        "portfolio": t("组合", "Portfolio", "ポートフォリオ", "포트폴리오", "Portfolio", "Portfolio", "Portfolio", "Portfolio", "Portfolio", "المحفظة"),
        "account": t("设置", "Settings", "設定", "설정", "Ajustes", "Paramètres", "Einstellungen", "Configurações", "Настройки", "الإعدادات"),
        "home": t("首页", "Home", "ホーム", "홈", "Inicio", "Accueil", "Start", "Início", "Главная", "الرئيسية"),
        "leaderboard": t("排行榜", "Leaderboard", "リーダーボード", "리더보드", "Ranking", "Classement", "Rangliste", "Ranking", "Рейтинг", "لوحة المتصدرين"),
        "watchlist": t("观察名单", "Watchlist", "ウォッチリスト", "관심 목록", "Lista de seguimiento", "Liste de suivi", "Watchlist", "Lista de observação", "Список наблюдения", "قائمة المراقبة"),
        "follows": t("我的跟随", "My follows", "フォロー一覧", "내 팔로우", "Mis seguimientos", "Mes suivis", "Meine Follows", "Meus seguimentos", "Мои подписки", "متابعاتي"),
        "dashboard": t("仪表盘", "Dashboard", "ダッシュボード", "대시보드", "Panel", "Tableau de bord", "Dashboard", "Painel", "Дашборд", "لوحة التحكم"),
        "portfolioPage": t("投资组合", "Portfolio", "ポートフォリオ", "포트폴리오", "Portfolio", "Portfolio", "Portfolio", "Portfolio", "Портфель", "المحفظة"),
        "wallet": t("钱包", "Wallet", "ウォレット", "지갑", "Wallet", "Wallet", "Wallet", "Wallet", "Кошелёк", "المحفظة"),
        "settings": t("设置", "Settings", "設定", "설정", "Ajustes", "Paramètres", "Einstellungen", "Configurações", "Настройки", "الإعدادات"),
        "connectWallet": t("连接钱包", "Connect wallet", "ウォレット接続", "지갑 연결", "Conectar wallet", "Connecter wallet", "Wallet verbinden", "Conectar wallet", "Подключить кошелёк", "ربط المحفظة"),
        "connectShort": t("连接", "Connect", "接続", "연결", "Conectar", "Connecter", "Verbinden", "Conectar", "Подключить", "ربط"),
        "connected": t("已连接", "Connected", "接続済み", "연결됨", "Conectado", "Connecté", "Verbunden", "Conectado", "Подключено", "متصل"),
        "disconnect": t("断开", "Disconnect", "切断", "연결 해제", "Desconectar", "Déconnecter", "Trennen", "Desconectar", "Отключить", "قطع الاتصال"),
        "language": t("语言", "Language", "言語", "언어", "Idioma", "Langue", "Sprache", "Idioma", "Язык", "اللغة"),
        "toggleTheme": t("切换主题", "Toggle theme", "テーマ切替", "테마 전환", "Cambiar tema", "Changer le thème", "Theme wechseln", "Alternar tema", "Сменить тему", "تبديل السمة"),
    }


def build_footer():
    return {
        "ariaNav": t("页脚导航", "Footer navigation", "フッターナビ", "푸터 내비게이션", "Navegación del pie", "Navigation du pied de page", "Fußzeilen-Navigation", "Navegação do rodapé", "Навигация в подвале", "تنقل التذييل"),
        "ariaContact": t("联系我们", "Contact", "お問い合わせ", "문의", "Contacto", "Contact", "Kontakt", "Contacto", "Контакты", "اتصل بنا"),
        "contact": t("联系我们", "Contact", "お問い合わせ", "문의", "Contacto", "Contact", "Kontakt", "Contacto", "Контакты", "اتصل بنا"),
        "tagline": t("多平台预测市场跟单", "Multi-venue prediction market copy trading", "マルチ Venue 予測市場コピー取引", "멀티 Venue 예측 시장 카피 트레이딩", "Copy trading en mercados de predicción multi-Venue", "Copy trading sur marchés prédictifs multi-Venue", "Copy-Trading an Multi-Venue-Prognosemärkten", "Copy trading em mercados de previsão multi-Venue", "Копитрейдинг на прогнозных рынках (multi-Venue)", "نسخ التداول في أسواق التوقعات متعددة المنصات"),
        "note": t("通道 A 为委托交易（尚未到完全非托管）；通道 B 为自托管 daemon 零钥（Pro+）。", "Channel A is delegated trading (not fully non-custodial yet); Channel B is self-hosted daemon zero-key (Pro+).", "チャネル A は委託取引（完全ノンカストディ未達）；チャネル B は自ホスト daemon ゼロキー（Pro+）。", "채널 A는 위탁 거래(완전 비수탁 미달); 채널 B는 자체 호스트 daemon 제로키(Pro+).", "Canal A: trading delegado (aún no totalmente no custodial); Canal B: daemon autoalojado zero-key (Pro+).", "Canal A : trading délégué (pas encore entièrement non custodial) ; Canal B : daemon auto-hébergé zero-key (Pro+).", "Kanal A: delegiertes Trading (noch nicht vollständig non-custodial); Kanal B: self-hosted daemon Zero-Key (Pro+).", "Canal A: trading delegado (ainda não totalmente non-custodial); Canal B: daemon self-hosted zero-key (Pro+).", "Канал A — делегированная торговля (ещё не полностью non-custodial); Канал B — self-hosted daemon zero-key (Pro+).", "القناة A تداول مفوض (ليس non-custodial بالكامل بعد)；القناة B daemon ذاتي zero-key (Pro+)."),
    }


def build_leaderboard():
    return {
        "title": t("排行榜", "Leaderboard", "リーダーボード", "리더보드", "Ranking", "Classement", "Rangliste", "Ranking", "Рейтинг", "لوحة المتصدرين"),
        "searchPlaceholder": t("地址 / alias / @x", "Address / alias / @x", "アドレス / alias / @x", "주소 / alias / @x", "Dirección / alias / @x", "Adresse / alias / @x", "Adresse / alias / @x", "Endereço / alias / @x", "Адрес / alias / @x", "عنوان / alias / @x"),
        "allPlatforms": t("全部平台", "All platforms", "全プラットフォーム", "전체 플랫폼", "Todas las plataformas", "Toutes les plateformes", "Alle Plattformen", "Todas as plataformas", "Все платформы", "جميع المنصات"),
        "allPlatformsShort": t("全平台", "All platforms", "全会場", "전체", "Todas", "Toutes", "Alle", "Todas", "Все", "الكل"),
        "sortDesc": t("降序", "Descending", "降順", "내림차순", "Descendente", "Décroissant", "Absteigend", "Decrescente", "По убыванию", "تنازلي"),
        "andFilters": t("共同筛选", "Combined filters", "複合フィルター", "복합 필터", "Filtros combinados", "Filtres combinés", "Kombinierte Filter", "Filtros combinados", "Комбинированные фильтры", "فلاتر مجمعة"),
        "hotOnly": t("仅热钥", "Hot only", "ホットのみ", "핫만", "Solo hot", "Hot uniquement", "Nur Hot", "Só hot", "Только hot", "Hot فقط"),
        "verifiedOnly": t("仅验证", "Verified only", "認証のみ", "인증만", "Solo verificados", "Vérifiés uniquement", "Nur verifiziert", "Só verificados", "Только verified", "موثق فقط"),
        "hideBots": t("隐藏机器人", "Hide bots", "ボットを隠す", "봇 숨기기", "Ocultar bots", "Masquer les bots", "Bots ausblenden", "Ocultar bots", "Скрыть ботов", "إخفاء البots"),
        "requirePerf": t("严格匹配周期/分类", "Strict period/category match", "期間/カテゴリ厳密一致", "기간/카테고리 엄격 일치", "Coincidencia estricta periodo/categoría", "Correspondance stricte période/catégorie", "Strikter Perioden/Kategorie-Abgleich", "Correspondência estrita período/categoria", "Строгое совпадение периода/категории", "تطابق صارم للفترة/الفئة"),
        "empty": t("无匹配交易者", "No matching traders", "一致するトレーダーがありません", "일치하는 트레이더 없음", "Sin traders coincidentes", "Aucun trader correspondant", "Keine passenden Trader", "Nenhum trader correspondente", "Нет подходящих трейдеров", "لا متداولين مطابقين"),
        "emptyStrict": t("无匹配交易者（共同筛选：当前周期/分类无绩效行的已剔除）", "No matching traders (strict filter removed those without performance for this period/category)", "一致するトレーダーがありません（厳密フィルターで当該期間/カテゴリの実績がないものを除外）", "일치하는 트레이더 없음(엄격 필터: 해당 기간/카테고리 실적 없는 항목 제외)", "Sin traders (filtro estricto eliminó los sin rendimiento en este periodo/categoría)", "Aucun trader (filtre strict : exclus sans performance pour cette période/catégorie)", "Keine Trader (Strikter Filter entfernte ohne Performance für diese Periode/Kategorie)", "Nenhum trader (filtro estrito removeu sem performance neste período/categoria)", "Нет трейдеров (строгий фильтр убрал без performance за период/категорию)", "لا متداولين (الفلتر الصارم أزال من بلا أداء لهذه الفترة/الفئة)"),
        "colRank": t("#", "#", "#", "#", "#", "#", "#", "#", "#", "#"),
        "colTrader": COL["trader"],
        "colSpark": t("曲线", "Chart", "チャート", "차트", "Gráfico", "Graphique", "Chart", "Gráfico", "График", "الرسم"),
        "colRoi": t("ROI", "ROI", "ROI", "ROI", "ROI", "ROI", "ROI", "ROI", "ROI", "ROI"),
        "colSharpe": t("Sharpe", "Sharpe", "Sharpe", "Sharpe", "Sharpe", "Sharpe", "Sharpe", "Sharpe", "Sharpe", "Sharpe"),
        "colWinRate": COL["winRate"],
        "colDrawdown": t("回撤", "Drawdown", "ドローダウン", "낙폭", "Drawdown", "Drawdown", "Drawdown", "Drawdown", "Просадка", "الانخفاض"),
        "colPnl": t("已实现", "Realized", "実現損益", "실현", "Realizado", "Réalisé", "Realisiert", "Realizado", "Реализовано", "محقق"),
        "colPlatform": t("平台", "Venue", "会場", "Venue", "Venue", "Venue", "Venue", "Venue", "Venue", "Venue"),
        "colTags": t("标签", "Tags", "タグ", "태그", "Etiquetas", "Tags", "Tags", "Tags", "Теги", "الوسوم"),
        "colBot": t("Bot", "Bot", "Bot", "Bot", "Bot", "Bot", "Bot", "Bot", "Bot", "Bot"),
        "colWatch": t("观察", "Watch", "監視", "관찰", "Observar", "Surveiller", "Beobachten", "Observar", "Наблюдать", "مراقبة"),
        "watchTitle": t("加入观察名单", "Add to watchlist", "ウォッチリストに追加", "관심 목록에 추가", "Añadir a watchlist", "Ajouter à la watchlist", "Zur Watchlist hinzufügen", "Adicionar à watchlist", "В список наблюдения", "إضافة لقائمة المراقبة"),
        "watchAdded": t("已加入观察名单", "Added to watchlist", "ウォッチリストに追加しました", "관심 목록에 추가됨", "Añadido a watchlist", "Ajouté à la watchlist", "Zur Watchlist hinzugefügt", "Adicionado à watchlist", "Добавлено в watchlist", "أُضيف لقائمة المراقبة"),
        "watchExists": t("已在观察名单中", "Already on watchlist", "すでにウォッチリストにあります", "이미 관심 목록에 있음", "Ya en watchlist", "Déjà dans la watchlist", "Bereits auf Watchlist", "Já na watchlist", "Уже в watchlist", "موجود في قائمة المراقبة"),
        "watchFailed": t("加入观察名单失败", "Failed to add to watchlist", "追加に失敗しました", "추가 실패", "Error al añadir a watchlist", "Échec de l'ajout à la watchlist", "Watchlist-Hinzufügen fehlgeschlagen", "Falha ao adicionar à watchlist", "Не удалось добавить в watchlist", "فشل الإضافة لقائمة المراقبة"),
        "botMarked": t("被 botfilter 标记为机器人", "Marked as bot by botfilter", "botfilter によりボットと判定", "botfilter가 봇으로 표시", "Marcado como bot por botfilter", "Marqué bot par botfilter", "Von botfilter als Bot markiert", "Marcado como bot pelo botfilter", "Помечен botfilter как bot", "مُعلَّم bot كبوت"),
        "botConfidence": t("botfilter 置信度", "botfilter confidence", "botfilter 信頼度", "botfilter 신뢰도", "Confianza botfilter", "Confiance botfilter", "botfilter-Konfidenz", "Confiança botfilter", "Уверенность botfilter", "ثقة botfilter"),
        "hotMark": t("热钥", "Hot key", "ホットキー", "핫 키", "Clave hot", "Clé hot", "Hot-Key", "Chave hot", "Hot key", "مفتاح hot"),
        "verifiedMark": t("已验证", "Verified", "認証済み", "인증됨", "Verificado", "Vérifié", "Verifiziert", "Verificado", "Verified", "موثق"),
        "showing": t("显示 {start}-{end} / {total}", "Showing {start}-{end} / {total}", "{start}-{end} / {total} を表示", "{start}-{end} / {total} 표시", "Mostrando {start}-{end} / {total}", "Affichage {start}-{end} / {total}", "Zeige {start}-{end} / {total}", "A mostrar {start}-{end} / {total}", "Показано {start}-{end} / {total}", "عرض {start}-{end} / {total}"),
        "showingPartial": t("显示 {start}-{end}", "Showing {start}-{end}", "{start}-{end} を表示", "{start}-{end} 표시", "Mostrando {start}-{end}", "Affichage {start}-{end}", "Zeige {start}-{end}", "A mostrar {start}-{end}", "Показано {start}-{end}", "عرض {start}-{end}"),
        "lastPage": t("（末页）", " (last page)", "（最終ページ）", " (마지막 페이지)", " (última página)", " (dernière page)", " (letzte Seite)", " (última página)", " (последняя)", " (الصفحة الأخيرة)"),
        "jumpTo": t("跳至", "Go to", "移動", "이동", "Ir a", "Aller à", "Gehe zu", "Ir para", "Перейти", "انتقل إلى"),
        "page": t("页", "page", "ページ", "페이지", "página", "page", "Seite", "página", "стр.", "صفحة"),
        "jump": t("跳转", "Go", "移動", "이동", "Ir", "Aller", "Los", "Ir", "Перейти", "انتقال"),
        "pageLabel": t("页码", "Page number", "ページ番号", "페이지 번호", "Número de página", "Numéro de page", "Seitennummer", "Número da página", "Номер страницы", "رقم الصفحة"),
        "searchTerm": t("搜索「{q}」", "Search “{q}”", "検索「{q}」", "검색「{q}」", "Buscar «{q}»", "Recherche « {q} »", "Suche „{q}“", "Pesquisar «{q}»", "Поиск «{q}»", "بحث «{q}»"),
        "peopleCount": t(" · {n} 人", " · {n} traders", " · {n} 人", " · {n}명", " · {n} traders", " · {n} traders", " · {n} Trader", " · {n} traders", " · {n} трейдеров", " · {n} متداول"),
        "sort": {
            "roi": t("ROI", "ROI", "ROI", "ROI", "ROI", "ROI", "ROI", "ROI", "ROI", "ROI"),
            "sharpe": t("Sharpe", "Sharpe", "Sharpe", "Sharpe", "Sharpe", "Sharpe", "Sharpe", "Sharpe", "Sharpe", "Sharpe"),
            "win_rate": COL["winRate"],
            "max_drawdown": t("回撤", "Drawdown", "ドローダウン", "낙폭", "Drawdown", "Drawdown", "Drawdown", "Drawdown", "Просадка", "الانخفاض"),
            "realized_pnl": t("已实现 PnL", "Realized PnL", "実現 PnL", "실현 PnL", "PnL realizado", "PnL réalisé", "Realisierter PnL", "PnL realizado", "Реализованный PnL", "PnL محقق"),
            "total_volume": t("成交量", "Volume", "出来高", "거래량", "Volumen", "Volume", "Volumen", "Volume", "Объём", "الحجم"),
            "updated_at": t("更新时间", "Updated", "更新日時", "업데이트", "Actualizado", "Mis à jour", "Aktualisiert", "Atualizado", "Обновлено", "محدّث"),
        },
        "period": PERIOD,
        "category": {
            "OVERALL": t("全部", "All", "すべて", "전체", "Todo", "Tout", "Alle", "Todos", "Все", "الكل"),
            "POLITICS": t("政治", "Politics", "政治", "정치", "Política", "Politique", "Politik", "Política", "Политика", "السياسة"),
            "SPORTS": t("体育", "Sports", "スポーツ", "스포츠", "Deportes", "Sports", "Sport", "Desporto", "Спорт", "الرياضة"),
            "ESPORTS": t("电竞", "Esports", "eスポーツ", "e스포츠", "Esports", "Esports", "Esports", "Esports", "Киберспорт", "الرياضات الإلكترونية"),
            "CRYPTO": t("加密", "Crypto", "暗号資産", "암호화폐", "Cripto", "Crypto", "Krypto", "Cripto", "Крипто", "العملات الرقمية"),
            "CULTURE": t("文化", "Culture", "カルチャー", "문화", "Cultura", "Culture", "Kultur", "Cultura", "Культура", "الثقافة"),
            "MENTIONS": t("提及", "Mentions", "言及", "언급", "Menciones", "Mentions", "Erwähnungen", "Menções", "Упоминания", "الإشارات"),
            "WEATHER": t("天气", "Weather", "天気", "날씨", "Clima", "Météo", "Wetter", "Clima", "Погода", "الطقس"),
            "ECONOMICS": t("经济", "Economics", "経済", "경제", "Economía", "Économie", "Wirtschaft", "Economia", "Экономика", "الاقتصاد"),
            "TECH": t("科技", "Tech", "テック", "기술", "Tecnología", "Tech", "Tech", "Tech", "Тех", "التقنية"),
            "FINANCE": t("金融", "Finance", "金融", "금융", "Finanzas", "Finance", "Finanzen", "Finanças", "Финансы", "المالية"),
        },
    }


def build_home():
    return {
        "title": t("多平台预测市场跟单", "Multi-venue prediction market copy trading", "マルチ Venue 予測市場コピー取引", "멀티 Venue 예측 시장 카피 트레이딩", "Copy trading en mercados de predicción multi-Venue", "Copy trading sur marchés prédictifs multi-Venue", "Copy-Trading an Multi-Venue-Prognosemärkten", "Copy trading em mercados de previsão multi-Venue", "Копитрейдинг на прогнозных рынках (multi-Venue)", "نسخ التداول في أسواق التوقعات متعددة المنصات"),
        "sub": t("发现高手，一键跟随，组合复盘。", "Find top traders, follow in one click, review your portfolio.", "トップトレーダーを見つけ、ワンクリックでフォロー、ポートフォリオを振り返る。", "탑 트레이더를 찾고 원클릭 팔로우, 포트폴리오 복기.", "Encuentra traders top, sigue con un clic, revisa tu portfolio.", "Trouvez les meilleurs traders, suivez en un clic, analysez votre portfolio.", "Top-Trader finden, mit einem Klick folgen, Portfolio analysieren.", "Encontre top traders, siga num clique, revise o portfolio.", "Найдите топ-трейдеров, следуйте в один клик, анализируйте портфель.", "اكتشف المتداولين المتميزين، تابع بنقرة، راجع محفظتك."),
        "discover": t("发现交易者", "Discover traders", "トレーダーを探す", "트레이더 발견", "Descubrir traders", "Découvrir des traders", "Trader entdecken", "Descobrir traders", "Найти трейдеров", "اكتشف المتداولين"),
        "goDashboard": t("进入仪表盘", "Go to dashboard", "ダッシュボードへ", "대시보드로", "Ir al panel", "Aller au tableau de bord", "Zum Dashboard", "Ir ao painel", "На дашборд", "إلى لوحة التحكم"),
        "connectWallet": t("连接钱包", "Connect wallet", "ウォレット接続", "지갑 연결", "Conectar wallet", "Connecter wallet", "Wallet verbinden", "Conectar wallet", "Подключить кошелёк", "ربط المحفظة"),
        "venue": {
            "cap": {
                "signalSource": t("信号", "Signal", "シグナル", "시그널", "Señal", "Signal", "Signal", "Sinal", "Сигнал", "إشارة"),
                "executionVenue": t("执行", "Execution", "実行", "실행", "Ejecución", "Exécution", "Ausführung", "Execução", "Исполнение", "التنفيذ"),
                "both": t("信号 + 执行", "Signal + execution", "シグナル + 実行", "시그널 + 실행", "Señal + ejecución", "Signal + exécution", "Signal + Ausführung", "Sinal + execução", "Сигнал + исполнение", "إشارة + تنفيذ"),
            },
            "auth": {"none": t("无需鉴权", "No auth required", "認証不要", "인증 불필요", "Sin autenticación", "Sans authentification", "Keine Auth nötig", "Sem autenticação", "Без аутентификации", "بدون مصادقة")},
            "geo": {"globalWithUsRestrictions": t("Global（美限制）", "Global (US restrictions)", "Global（米国制限）", "Global(미국 제한)", "Global (restricciones EE.UU.)", "Global (restrictions US)", "Global (US-Beschränkungen)", "Global (restrições EUA)", "Global (ограничения США)", "Global (قيود أمريكية)")},
            "phaseDefault": t("即将开放", "Coming soon", "近日公開", "곧 공개", "Próximamente", "Bientôt", "Demnächst", "Em breve", "Скоро", "قريباً"),
            "liveBadge": t("已上线", "Live", "稼働中", "라이브", "En vivo", "En ligne", "Live", "Ativo", "Активно", "متاح"),
            "ariaLocked": t("{name}，{phase}，即将开放", "{name}, {phase}, coming soon", "{name}、{phase}、近日公開", "{name}, {phase}, 곧 공개", "{name}, {phase}, próximamente", "{name}, {phase}, bientôt", "{name}, {phase}, demnächst", "{name}, {phase}, em breve", "{name}, {phase}, скоро", "{name}، {phase}، قريباً"),
            "toastComingSoon": t("{name}（{phase}）即将开放", "{name} ({phase}) coming soon", "{name}（{phase}）近日公開", "{name}({phase}) 곧 공개", "{name} ({phase}) próximamente", "{name} ({phase}) bientôt", "{name} ({phase}) demnächst", "{name} ({phase}) em breve", "{name} ({phase}) скоро", "{name} ({phase}) قريباً"),
            "lockOverlay": t("即将开放 · 路线图", "Coming soon · roadmap", "近日公開 · ロードマップ", "곧 공개 · 로드맵", "Próximamente · hoja de ruta", "Bientôt · feuille de route", "Demnächst · Roadmap", "Em breve · roadmap", "Скоро · roadmap", "قريباً · خارطة الطريق"),
            "ariaLive": t("{name}，已上线，查看排行榜", "{name}, live, view leaderboard", "{name}、稼働中、リーダーボードを見る", "{name}, 라이브, 리더보드 보기", "{name}, en vivo, ver ranking", "{name}, en ligne, voir le classement", "{name}, live, Rangliste ansehen", "{name}, ativo, ver ranking", "{name}, активен, рейтинг", "{name}، متاح، عرض لوحة المتصدرين"),
        },
        "venues": {
            "sectionTitle": t("已接入与即将开放", "Live and coming soon", "稼働中と近日公開", "연결됨 및 곧 공개", "Activos y próximamente", "En ligne et bientôt", "Live und demnächst", "Ativos e em breve", "Активные и скоро", "متاح وقريباً"),
            "sectionDesc": t("一个终端，覆盖多个预测市场。", "One terminal, many prediction markets.", "1つのターミナルで複数の予測市場。", "하나의 터미널, 여러 예측 시장.", "Un terminal, muchos mercados de predicción.", "Un terminal, plusieurs marchés prédictifs.", "Ein Terminal, viele Prognosemärkte.", "Um terminal, vários mercados de previsão.", "Один терминал — множество рынков.", "محطة واحدة، أسواق توقعات متعددة."),
            "emptyTitle": t("暂无已接入 Venue", "No live Venues yet", "稼働中 Venue なし", "연결된 Venue 없음", "Sin Venues activos", "Aucun Venue en ligne", "Noch keine live Venues", "Sem Venues ativos", "Нет активных Venue", "لا Venue متاح"),
            "emptyHint": t("接入完成后，将在此展示可跟单的市场。", "Once integrated, copy-tradable markets will appear here.", "接続完了後、ここにコピー可能な市場が表示されます。", "연결 완료 후 카피 가능한 시장이 표시됩니다.", "Tras la integración, los mercados copiables aparecerán aquí.", "Une fois intégrés, les marchés copiables s'afficheront ici.", "Nach Integration erscheinen kopierbare Märkte hier.", "Após integração, mercados copiáveis aparecerão aqui.", "После интеграции здесь появятся рынки для копирования.", "بعد الربط، ستظهر الأسواق القابلة للنسخ هنا."),
            "loadError": t("Venue 列表暂时无法加载", "Venue list temporarily unavailable", "Venue リストを読み込めません", "Venue 목록 로드 불가", "Lista de Venues no disponible", "Liste Venue indisponible", "Venue-Liste nicht verfügbar", "Lista Venue indisponível", "Список Venue недоступен", "قائمة Venue غير متاحة"),
            "retry": t("重试", "Retry", "再試行", "재시도", "Reintentar", "Réessayer", "Erneut", "Tentar novamente", "Повторить", "إعادة المحاولة"),
        },
        "channels": {
            "ctaSetupFollows": t("去设置跟随", "Set up follows", "フォロー設定へ", "팔로우 설정", "Configurar seguimientos", "Configurer les suivis", "Follows einrichten", "Configurar seguimentos", "Настроить follows", "إعداد المتابعات"),
            "ctaConnectStart": t("连接钱包开始", "Connect wallet to start", "ウォレット接続で開始", "지갑 연결 후 시작", "Conectar wallet para empezar", "Connecter wallet pour commencer", "Wallet verbinden zum Start", "Conectar wallet para começar", "Подключите кошелёк", "اربط المحفظة للبدء"),
            "ctaConnectUpgrade": t("连接钱包后升级", "Connect wallet to upgrade", "接続後アップグレード", "연결 후 업그레이드", "Conectar wallet para mejorar", "Connecter wallet pour upgrader", "Wallet verbinden zum Upgrade", "Conectar wallet para upgrade", "Подключите кошелёк для upgrade", "اربط المحفظة للترقية"),
            "ctaConfigureDaemon": t("配置 Daemon", "Configure Daemon", "Daemon 設定", "Daemon 구성", "Configurar Daemon", "Configurer Daemon", "Daemon konfigurieren", "Configurar Daemon", "Настроить Daemon", "إعداد Daemon"),
            "ctaUpgradePro": t("升级 Pro+", "Upgrade Pro+", "Pro+ にアップグレード", "Pro+ 업그레이드", "Mejorar a Pro+", "Passer à Pro+", "Auf Pro+ upgraden", "Upgrade Pro+", "Upgrade Pro+", "ترقية Pro+"),
            "sectionTitle": t("两种跟单方式", "Two copy-trading paths", "2つのコピー方式", "두 가지 카피 방식", "Dos formas de copy trading", "Deux modes de copy trading", "Zwei Copy-Trading-Wege", "Dois modos de copy trading", "Два способа копитрейдинга", "طريقتان للنسخ"),
            "sectionDesc": t("按你需要的控制权选择。托管等级我们写清楚，不模糊宣传。", "Choose the control level you need. We state custody clearly—no vague marketing.", "必要なコントロールで選択。カストディは明確に記載。", "필요한 통제 수준 선택. 수탁 등급을 명확히.", "Elige el nivel de control. Custodia clara, sin marketing vago.", "Choisissez votre niveau de contrôle. Custodie claire, sans flou.", "Wählen Sie Ihr Kontrollniveau. Custody klar benannt.", "Escolha o nível de controlo. Custódia clara.", "Выберите уровень контроля. Custody прозрачно.", "اختر مستوى التحكم. نحدد Custody بوضوح."),
            "a": {
                "tag": t("通道 A", "Channel A", "チャネル A", "채널 A", "Canal A", "Canal A", "Kanal A", "Canal A", "Канал A", "القناة A"),
                "title": t("TG Deposit Wallet 委托代签", "TG Deposit Wallet delegated signing", "TG Deposit Wallet 委託代行", "TG Deposit Wallet 위탁 서명", "Firma delegada TG Deposit Wallet", "Signature déléguée TG Deposit Wallet", "Delegierte Signatur TG Deposit Wallet", "Assinatura delegada TG Deposit Wallet", "Делегированная подпись TG Deposit Wallet", "توقيع مفوض TG Deposit Wallet"),
                "lead": t("登录即可用，适合先跟起来。", "Sign in and start—good for getting started quickly.", "ログインですぐ使える。", "로그인 후 바로 사용.", "Inicia sesión y empieza.", "Connectez-vous et démarrez.", "Einloggen und loslegen.", "Entre e comece.", "Войдите и начните.", "سجّل الدخول وابدأ."),
                "point1": t("钱包登录后即可跟随", "Follow after wallet login", "ウォレットログイン後フォロー", "지갑 로그인 후 팔로우", "Seguir tras login wallet", "Suivre après connexion wallet", "Nach Wallet-Login folgen", "Seguir após login wallet", "Follow после входа", "متابعة بعد ربط المحفظة"),
                "point2": t("经 Telegram / 网页发起跟单", "Copy via Telegram / web", "Telegram / Web からコピー", "Telegram/웹으로 카피", "Copy vía Telegram / web", "Copy via Telegram / web", "Copy via Telegram / Web", "Copy via Telegram / web", "Copy через Telegram / web", "نسخ عبر Telegram / الويب"),
                "point3": t("平台代签执行（免自管私钥）", "Platform signs for you (no self-managed keys)", "プラットフォーム代行（自己鍵不要）", "플랫폼 대행(자체 키 불필요)", "Plataforma firma por ti", "Plateforme signe pour vous", "Plattform signiert für Sie", "Plataforma assina por si", "Платформа подписывает", "المنصة توقّع نيابةً عنك"),
                "custody": t("⚠ 委托交易 · 尚未到完全非托管", "⚠ Delegated trading · not fully non-custodial yet", "⚠ 委託取引 · 完全ノンカストディ未達", "⚠ 위탁 거래 · 완전 비수탁 미달", "⚠ Trading delegado · aún no totalmente non-custodial", "⚠ Trading délégué · pas encore entièrement non custodial", "⚠ Delegiertes Trading · noch nicht voll non-custodial", "⚠ Trading delegado · ainda não totalmente non-custodial", "⚠ Делегированная торговля · ещё не полностью non-custodial", "⚠ تداول مفوض · ليس non-custodial بالكامل"),
            },
            "b": {
                "tag": t("通道 B · Pro+", "Channel B · Pro+", "チャネル B · Pro+", "채널 B · Pro+", "Canal B · Pro+", "Canal B · Pro+", "Kanal B · Pro+", "Canal B · Pro+", "Канал B · Pro+", "القناة B · Pro+"),
                "title": t("自托管 Daemon 零钥", "Self-hosted Daemon zero-key", "自ホスト Daemon ゼロキー", "자체 호스트 Daemon 제로키", "Daemon autoalojado zero-key", "Daemon auto-hébergé zero-key", "Self-hosted Daemon Zero-Key", "Daemon self-hosted zero-key", "Self-hosted Daemon zero-key", "Daemon ذاتي zero-key"),
                "lead": t("私钥留在你这边，适合要更高控制权的人。", "Keys stay with you—for those who want more control.", "鍵はあなた側。より高いコントロール向け。", "키는 사용자 측. 더 높은 통제 원하는 분.", "Claves contigo—más control.", "Clés chez vous—plus de contrôle.", "Schlüssel bei Ihnen—mehr Kontrolle.", "Chaves consigo—mais controlo.", "Ключи у вас—больше контроля.", "المفاتيح لديك—تحكم أعلى."),
                "point1": t("本机 / 自建 daemon 执行", "Local / self-hosted daemon execution", "ローカル/自ホスト daemon 実行", "로컬/자체 daemon 실행", "Ejecución daemon local/autoalojado", "Exécution daemon local/auto-hébergé", "Lokaler/self-hosted daemon", "Execução daemon local/self-hosted", "Локальный/self-hosted daemon", "تنفيذ daemon محلي/ذاتي"),
                "point2": t("平台不持有交易私钥", "Platform never holds trading keys", "プラットフォームは取引鍵を保持しない", "플랫폼은 거래 키 미보유", "Plataforma no guarda claves", "Plateforme ne détient pas les clés", "Plattform hält keine Trading-Keys", "Plataforma não detém chaves", "Платформа не хранит ключи", "المنصة لا تحتفظ بمفاتيح التداول"),
                "point3": t("可跨 Venue、高级风控、无限跟随槽位", "Cross-Venue, advanced risk controls, unlimited follow slots", "クロス Venue、高度リスク管理、無制限スロット", "크로스 Venue, 고급 리스크, 무제한 슬롯", "Cross-Venue, riesgo avanzado, slots ilimitados", "Cross-Venue, risque avancé, slots illimités", "Cross-Venue, erweitertes Risiko, unbegrenzte Slots", "Cross-Venue, risco avançado, slots ilimitados", "Cross-Venue, расширенный риск, безлимит слотов", "Cross-Venue، مخاطر متقدمة، slots غير محدود"),
                "custody": t("✓ 执行侧零钥 · 需 Pro+", "✓ Zero-key execution · Pro+ required", "✓ 実行側ゼロキー · Pro+ 必要", "✓ 실행측 제로키 · Pro+ 필요", "✓ Ejecución zero-key · requiere Pro+", "✓ Exécution zero-key · Pro+ requis", "✓ Zero-Key-Ausführung · Pro+ nötig", "✓ Execução zero-key · Pro+ necessário", "✓ Zero-key исполнение · нужен Pro+", "✓ تنفيذ zero-key · يتطلب Pro+"),
            },
        },
        "closing": {
            "ctaDiscover": t("发现交易者", "Discover traders", "トレーダーを探す", "트레이더 발견", "Descubrir traders", "Découvrir des traders", "Trader entdecken", "Descobrir traders", "Найти трейдеров", "اكتشف المتداولين"),
            "ctaConnect": t("连接钱包", "Connect wallet", "ウォレット接続", "지갑 연결", "Conectar wallet", "Connecter wallet", "Wallet verbinden", "Conectar wallet", "Подключить кошелёк", "ربط المحفظة"),
            "title": t("选好要跟的人，开始复制他们的交易。", "Pick who to follow and copy their trades.", "フォローする人を選び、取引をコピー。", "팔로우할 사람을 고르고 거래를 복사.", "Elige a quién seguir y copia sus trades.", "Choisissez qui suivre et copiez leurs trades.", "Wählen Sie wen Sie folgen und kopieren Sie Trades.", "Escolha quem seguir e copie trades.", "Выберите кого следовать и копируйте сделки.", "اختر من تتابع وانسخ صفقاتهم."),
        },
        "hot": {
            "sectionTitle": t("热门交易者", "Hot traders", "人気トレーダー", "인기 트레이더", "Traders populares", "Traders populaires", "Hot Trader", "Traders populares", "Горячие трейдеры", "متداولون رائجون"),
            "sectionDesc": t("按近 30 日 ROI 排序的预览。", "Preview sorted by 30-day ROI.", "直近30日 ROI 順プレビュー。", "최근 30일 ROI 순 미리보기.", "Vista previa por ROI 30 días.", "Aperçu trié par ROI 30 jours.", "Vorschau nach 30-Tage-ROI.", "Pré-visualização por ROI 30 dias.", "Превью по ROI за 30 дней.", "معاينة حسب ROI 30 يوماً."),
            "viewLeaderboard": t("查看完整排行榜 →", "View full leaderboard →", "全ランキング →", "전체 리더보드 →", "Ver ranking completo →", "Voir le classement complet →", "Volle Rangliste →", "Ver ranking completo →", "Полный рейтинг →", "لوحة المتصدرين الكاملة →"),
            "emptyTitle": t("暂无热门交易者", "No hot traders yet", "人気トレーダーなし", "인기 트레이더 없음", "Sin traders populares", "Aucun trader populaire", "Keine Hot Trader", "Sem traders populares", "Нет hot трейдеров", "لا متداولين رائجين"),
            "emptyAction": t("去排行榜看看 →", "Browse leaderboard →", "ランキングを見る →", "리더보드 보기 →", "Ver ranking →", "Voir le classement →", "Rangliste ansehen →", "Ver ranking →", "Смотреть рейтинг →", "تصفح لوحة المتصدرين →"),
            "colTrader": COL["trader"],
            "colPlatform": COL["platform"],
            "colWinRate": COL["winRate"],
            "loadError": t("热门列表暂时无法加载", "Hot list temporarily unavailable", "人気リスト読み込み不可", "인기 목록 로드 불가", "Lista popular no disponible", "Liste populaire indisponible", "Hot-Liste nicht verfügbar", "Lista popular indisponível", "Hot-список недоступен", "القائمة الرائجة غير متاحة"),
            "retry": t("重试", "Retry", "再試行", "재시도", "Reintentar", "Réessayer", "Erneut", "Tentar novamente", "Повторить", "إعادة المحاولة"),
        },
    }


def _flat(*entries):
    """entries: list of (path, 10-tuple)"""
    out = {}
    for path, vals in entries:
        parts = path.split(".")
        d = out
        for part in parts[:-1]:
            d = d.setdefault(part, {})
        d[parts[-1]] = t(*vals)
    return out


def build_settings():
    E = lambda m: ("加载失败：{message}", f"Failed to load: {m}", f"読み込み失敗：{m}", f"로드 실패: {m}", f"Error al cargar: {m}", f"Échec : {m}", f"Laden fehlgeschlagen: {m}", f"Falha: {m}", f"Ошибка: {m}", f"فشل: {m}")
    return _flat(
        ("pageTitle", ("账户设置", "Account settings", "アカウント設定", "계정 설정", "Ajustes de cuenta", "Paramètres du compte", "Kontoeinstellungen", "Configurações da conta", "Настройки аккаунта", "إعدادات الحساب")),
        ("pageSubtitle", ("订阅、凭证与跟单通道配置", "Subscription, credentials, and copy channels", "サブスク・凭证・コピーチャネル", "구독, 자격증명, 카피 채널", "Suscripción, credenciales y canales", "Abonnement, identifiants et canaux", "Abo, Credentials und Kanäle", "Subscrição, credenciais e canais", "Подписка, credentials и каналы", "الاشتراك والاعتمادات والقنوات")),
        ("hub.subscription.title", ("订阅", "Subscription", "サブスク", "구독", "Suscripción", "Abonnement", "Abo", "Subscrição", "Подписка", "الاشتراك")),
        ("hub.subscription.desc", ("档位与权益", "Tiers and benefits", "プランと特典", "등급 및 혜택", "Planes y beneficios", "Offres et avantages", "Stufen und Vorteile", "Planos e benefícios", "Тарифы и benefits", "المستويات والمزايا")),
        ("hub.credentials.title", ("Venue 凭证", "Venue credentials", "Venue 凭证", "Venue 자격증명", "Credenciales Venue", "Identifiants Venue", "Venue-Credentials", "Credenciais Venue", "Venue credentials", "اعتمادات Venue")),
        ("hub.credentials.desc", ("交易授权", "Trading authorization", "取引授权", "거래 권한", "Autorización de trading", "Autorisation de trading", "Handelsautorisierung", "Autorização de trading", "Торговая авторизация", "تفويض التداول")),
        ("hub.delegation.title", ("委托", "Delegation", "委託", "위탁", "Delegación", "Délégation", "Delegation", "Delegação", "Делегирование", "التفويض")),
        ("hub.delegation.desc", ("托管与代理", "Custody and proxy", "カストディと代理", "수탁 및 대리", "Custodia y proxy", "Custodie et proxy", "Custody und Proxy", "Custódia e proxy", "Custody и proxy", "الحفظ والوكالة")),
        ("hub.daemonKey.desc", ("自托管通道", "Self-hosted channel", "自ホストチャネル", "자체 호스트 채널", "Canal autoalojado", "Canal auto-hébergé", "Self-hosted Kanal", "Canal self-hosted", "Self-hosted канал", "قناة ذاتية")),
        ("account.sectionTitle", ("账户", "Account", "アカウント", "계정", "Cuenta", "Compte", "Konto", "Conta", "Аккаунт", "الحساب")),
        ("account.connectedWallet", ("已连接钱包：", "Connected wallet:", "接続ウォレット：", "연결된 지갑:", "Wallet conectada:", "Wallet connectée :", "Verbundene Wallet:", "Wallet conectada:", "Подключённый кошелёк:", "المحفظة المتصلة:")),
        ("account.userId", ("用户 ID：{id}", "User ID: {id}", "ユーザー ID：{id}", "사용자 ID: {id}", "ID de usuario: {id}", "ID utilisateur : {id}", "Benutzer-ID: {id}", "ID do utilizador: {id}", "ID пользователя: {id}", "معرف المستخدم: {id}")),
        ("account.loadError", E("message")),
        ("subscription.sectionTitle", ("订阅", "Subscription", "サブスク", "구독", "Suscripción", "Abonnement", "Abo", "Subscrição", "Подписка", "الاشتراك")),
        ("subscription.manageLink", ("管理 →", "Manage →", "管理 →", "관리 →", "Gestionar →", "Gérer →", "Verwalten →", "Gerir →", "Управление →", "إدارة →")),
        ("subscription.currentTier", ("当前档位：", "Current tier:", "現在のプラン：", "현재 등급:", "Plan actual:", "Offre actuelle :", "Aktuelle Stufe:", "Plano atual:", "Текущий тариф:", "المستوى الحالي:")),
        ("subscription.until", ("订阅至 {date}", "Subscribed until {date}", "有効期限 {date}", "{date}까지 구독", "Suscripción hasta {date}", "Abonné jusqu'au {date}", "Abo bis {date}", "Subscrição até {date}", "Подписка до {date}", "اشتراك حتى {date}")),
        ("subscription.upgradeLink", ("升级 / 管理订阅 →", "Upgrade / manage subscription →", "アップグレード / 管理 →", "업그레이드 / 관리 →", "Mejorar / gestionar suscripción →", "Upgrade / gérer abonnement →", "Upgrade / Abo verwalten →", "Upgrade / gerir subscrição →", "Upgrade / управление →", "ترقية / إدارة الاشتراك →")),
        ("subscription.loadError", E("message")),
        ("credentials.sectionTitle", ("Venue 凭证", "Venue credentials", "Venue 凭证", "Venue 자격증명", "Credenciales Venue", "Identifiants Venue", "Venue-Credentials", "Credenciais Venue", "Venue credentials", "اعتمادات Venue")),
        ("credentials.emptyTitle", ("尚未配置任何 Venue 凭证", "No Venue credentials configured", "Venue 凭证未設定", "Venue 자격증명 미설정", "Sin credenciales Venue", "Aucun identifiant Venue", "Keine Venue-Credentials", "Sem credenciais Venue", "Venue credentials не настроены", "لا اعتمادات Venue")),
        ("credentials.emptyAction", ("去配置 →", "Configure →", "設定へ →", "설정 →", "Configurar →", "Configurer →", "Konfigurieren →", "Configurar →", "Настроить →", "إعداد →")),
        ("credentials.proxyAddress", (" · 代理地址：{address}", " · Proxy: {address}", " · プロキシ：{address}", " · 프록시: {address}", " · Proxy: {address}", " · Proxy : {address}", " · Proxy: {address}", " · Proxy: {address}", " · Proxy: {address}", " · الوكيل: {address}")),
        ("credentials.loadError", E("message")),
        ("daemonKey.manageLink", ("管理 →", "Manage →", "管理 →", "관리 →", "Gestionar →", "Gérer →", "Verwalten →", "Gerir →", "Управление →", "إدارة →")),
        ("daemonKey.description", ("用于自托管 daemon 拉取跟单（通道B）。轮换后旧 key 立即失效，明文仅显示一次。", "For self-hosted daemon copy pull (Channel B). Old keys invalidate on rotate; plaintext shown once.", "自ホスト daemon 用（チャネルB）。ローテートで旧 key 無効、平文は一度のみ。", "자체 daemon 카피(채널B). 로테이트 시 구 key 무효, 평문 1회.", "Para daemon autoalojado (Canal B). Rotación invalida key anterior; texto plano una vez.", "Pour daemon auto-hébergé (Canal B). Rotation invalide l'ancienne clé ; texte clair une fois.", "Für self-hosted daemon (Kanal B). Rotation invalidiert alten Key; Klartext einmal.", "Para daemon self-hosted (Canal B). Rotação invalida key antiga; texto uma vez.", "Для self-hosted daemon (Канал B). Ротация аннулирует старый key; plaintext один раз.", "لـ daemon ذاتي (القناة B). التدوير يلغي المفتاح القديم؛ النص مرة واحدة.")),
        ("daemonKey.gotoLink", ("前往 daemon key 管理 →", "Go to daemon key management →", "daemon key 管理へ →", "daemon key 관리 →", "Ir a gestión daemon key →", "Gestion des clés daemon →", "Daemon-Key-Verwaltung →", "Gestão daemon key →", "Управление daemon key →", "إدارة daemon key →")),
    )


def build_ui():
    return {
        "emptyDefault": t("暂无数据", "No data yet", "データなし", "데이터 없음", "Sin datos", "Aucune donnée", "Keine Daten", "Sem dados", "Нет данных", "لا بيانات"),
        "errorTitle": t("出错了", "Something went wrong", "エラー", "오류", "Error", "Erreur", "Fehler", "Erro", "Ошибка", "خطأ"),
        "periodLabel": COL["periodLabel"],
    }


def build_errors():
    return {
        "network": t("网络错误：{msg}", "Network error: {msg}", "ネットワークエラー：{msg}", "네트워크 오류: {msg}", "Error de red: {msg}", "Erreur réseau : {msg}", "Netzwerkfehler: {msg}", "Erro de rede: {msg}", "Сетевая ошибка: {msg}", "خطأ شبكة: {msg}"),
        "sessionExpired": t("连接已过期，请重新连接钱包", "Session expired—reconnect your wallet", "接続期限切れ—ウォレット再接続", "연결 만료—지갑 재연결", "Sesión expirada—reconecta wallet", "Session expirée—reconnectez wallet", "Sitzung abgelaufen—Wallet neu verbinden", "Sessão expirada—reconecte wallet", "Сессия истекла—переподключите кошелёк", "انتهت الجلسة—أعد ربط المحفظة"),
        "serviceUnavailable": t("服务暂不可用，请稍后重试", "Service temporarily unavailable", "サービス一時不可", "서비스 일시 불가", "Servicio no disponible", "Service indisponible", "Dienst nicht verfügbar", "Serviço indisponível", "Сервис недоступен", "الخدمة غير متاحة"),
    }


def build_wallet_connect():
    return {
        "title": t("连接钱包", "Connect wallet", "ウォレット接続", "지갑 연결", "Conectar wallet", "Connecter wallet", "Wallet verbinden", "Conectar wallet", "Подключить кошелёк", "ربط المحفظة"),
        "hint": t("选择已安装的浏览器扩展钱包", "Choose an installed browser extension wallet", "インストール済み拡張ウォレットを選択", "설치된 브라우저 지갑 선택", "Elige una wallet de extensión instalada", "Choisissez une extension wallet installée", "Installierte Browser-Wallet wählen", "Escolha uma wallet de extensão", "Выберите установленный wallet", "اختر محفظة امتداد مثبتة"),
        "detecting": t("正在检测已安装的钱包…", "Detecting installed wallets…", "ウォレット検出中…", "지갑 감지 중…", "Detectando wallets…", "Détection des wallets…", "Wallets werden erkannt…", "A detetar wallets…", "Поиск wallets…", "جاري اكتشاف المحافظ…"),
        "connect": t("连接", "Connect", "接続", "연결", "Conectar", "Connecter", "Verbinden", "Conectar", "Подключить", "ربط"),
        "noWallets": t("未检测到浏览器扩展钱包", "No browser extension wallets detected", "拡張ウォレット未検出", "확장 지갑 없음", "Sin wallets de extensión", "Aucune extension wallet", "Keine Browser-Wallets", "Sem wallets de extensão", "Нет extension wallets", "لم تُكتشف محافظ"),
        "installHint": t("请安装 MetaMask / TokenPocket / Rabby / OKX 等后刷新页面", "Install MetaMask / TokenPocket / Rabby / OKX etc. and refresh", "MetaMask 等をインストールして更新", "MetaMask 등 설치 후 새로고침", "Instala MetaMask / TokenPocket / Rabby / OKX y recarga", "Installez MetaMask / TokenPocket / Rabby / OKX puis actualisez", "MetaMask etc. installieren und neu laden", "Instale MetaMask / TokenPocket / Rabby / OKX e atualize", "Установите MetaMask и обновите", "ثبّت MetaMask وغيرها ثم حدّث"),
        "unknownWallet": t("未知钱包", "Unknown wallet", "不明なウォレット", "알 수 없는 지갑", "Wallet desconocida", "Wallet inconnue", "Unbekannte Wallet", "Wallet desconhecida", "Неизвестный wallet", "محفظة غير معروفة"),
        "connectedToast": t("钱包已连接", "Wallet connected", "ウォレット接続済み", "지갑 연결됨", "Wallet conectada", "Wallet connectée", "Wallet verbunden", "Wallet conectada", "Кошелёк подключён", "تم ربط المحفظة"),
    }


def build_siwe():
    return {
        "injectedWallet": t("注入钱包", "Injected wallet", "注入ウォレット", "주입된 지갑", "Wallet inyectada", "Wallet injectée", "Injectierte Wallet", "Wallet injetada", "Injected wallet", "محفظة مدمجة"),
        "statement": t("Connect wallet to Sharpside", "Connect wallet to Sharpside", "Connect wallet to Sharpside", "Connect wallet to Sharpside", "Connect wallet to Sharpside", "Connect wallet to Sharpside", "Connect wallet to Sharpside", "Connect wallet to Sharpside", "Connect wallet to Sharpside", "Connect wallet to Sharpside"),
    }


def build_one_time_secret():
    return {
        "defaultTitle": t("明文密钥（仅显示一次）", "Secret (shown once)", "シークレット（一度のみ）", "시크릿(1회)", "Secreto (una vez)", "Secret (une fois)", "Secret (einmal)", "Segredo (uma vez)", "Секрет (один раз)", "سر (مرة واحدة)"),
        "defaultWarn": t("请立即妥善保存，关闭后无法再次查看。", "Save it now—you cannot view it again after closing.", "今すぐ保存。閉じると再表示不可。", "지금 저장. 닫으면 다시 볼 수 없음.", "Guárdalo ahora—no podrás verlo de nuevo.", "Sauvegardez maintenant—plus visible après fermeture.", "Jetzt speichern—nach Schließen nicht mehr sichtbar.", "Guarde agora—não verá de novo.", "Сохраните сейчас—после закрытия не показать.", "احفظه الآن—لن تراه مجدداً."),
        "savedConfirm": t("我已妥善保存此密钥", "I have saved this secret", "保存しました", "저장했습니다", "He guardado el secreto", "J'ai sauvegardé", "Ich habe gespeichert", "Guardei o segredo", "Я сохранил секрет", "حفظت هذا السر"),
    }


def build_connect():
    return {"redirectOnly": t("正在打开钱包连接…", "Opening wallet connect…", "ウォレット接続を開いています…", "지갑 연결 여는 중…", "Abriendo conexión wallet…", "Ouverture connexion wallet…", "Wallet-Verbindung wird geöffnet…", "A abrir conexão wallet…", "Открытие подключения wallet…", "جاري فتح ربط المحفظة…")}


def build_import_page():
    return _flat(
        ("title", ("导入交易者", "Import trader", "トレーダーインポート", "트레이더 가져오기", "Importar trader", "Importer trader", "Trader importieren", "Importar trader", "Импорт трейдера", "استيراد متداول")),
        ("desc", ("输入钱包地址，触发成交回填并加入观察名单。", "Enter wallet address to backfill trades and add to watchlist.", "アドレス入力で約定回填・ウォッチリスト追加。", "주소 입력으로 체결 백필 및 관심 목록 추가.", "Introduce dirección para rellenar trades y añadir a watchlist.", "Entrez l'adresse pour backfill et watchlist.", "Adresse eingeben für Backfill und Watchlist.", "Introduza endereço para backfill e watchlist.", "Введите адрес для backfill и watchlist.", "أدخل العنوان للملء وقائمة المراقبة.")),
        ("platformLabel", ("平台", "Platform", "プラットフォーム", "플랫폼", "Plataforma", "Plateforme", "Plattform", "Plataforma", "Платформа", "المنصة")),
        ("addressLabel", ("钱包地址", "Wallet address", "ウォレットアドレス", "지갑 주소", "Dirección wallet", "Adresse wallet", "Wallet-Adresse", "Endereço wallet", "Адрес кошелька", "عنوان المحفظة")),
        ("addressPlaceholder", ("0x…  (Polymarket proxy wallet)", "0x…  (Polymarket proxy wallet)", "0x…  (Polymarket proxy wallet)", "0x…  (Polymarket proxy wallet)", "0x…  (Polymarket proxy wallet)", "0x…  (Polymarket proxy wallet)", "0x…  (Polymarket proxy wallet)", "0x…  (Polymarket proxy wallet)", "0x…  (Polymarket proxy wallet)", "0x…  (Polymarket proxy wallet)")),
        ("submit", ("导入并回填", "Import & backfill", "インポート＆回填", "가져오기 및 백필", "Importar y rellenar", "Importer et backfill", "Importieren & backfill", "Importar e backfill", "Импорт и backfill", "استيراد وملء")),
        ("backfilling", ("回填中…", "Backfilling…", "回填中…", "백필 중…", "Rellenando…", "Backfill…", "Backfill…", "A preencher…", "Backfill…", "جاري الملء…")),
        ("errorPlatform", ("请选择平台", "Select a platform", "プラットフォームを選択", "플랫폼 선택", "Selecciona plataforma", "Sélectionnez une plateforme", "Plattform wählen", "Selecione plataforma", "Выберите платформу", "اختر المنصة")),
        ("errorAddress", ("请填写钱包地址", "Enter wallet address", "アドレスを入力", "주소 입력", "Introduce dirección", "Entrez l'adresse", "Adresse eingeben", "Introduza endereço", "Введите адрес", "أدخل العنوان")),
        ("errorFormat", ("地址格式不正确，应为 0x 开头的 40 位 hex", "Invalid address—must be 0x + 40 hex chars", "アドレス形式が不正（0x+40hex）", "주소 형식 오류(0x+40hex)", "Dirección inválida—0x + 40 hex", "Adresse invalide—0x + 40 hex", "Ungültige Adresse—0x + 40 hex", "Endereço inválido—0x + 40 hex", "Неверный адрес—0x + 40 hex", "عنوان غير صالح—0x + 40 hex")),
        ("success", ("导入成功，回填 {n} 笔成交", "Imported, backfilled {n} trades", "インポート成功、{n} 件回填", "가져오기 성공, {n}건 백필", "Importado, {n} trades rellenados", "Importé, {n} trades backfill", "Importiert, {n} Trades backfill", "Importado, {n} trades", "Импорт, backfill {n} сделок", "استيراد، ملء {n} صفقة")),
        ("failed", ("导入失败", "Import failed", "インポート失敗", "가져오기 실패", "Importación fallida", "Échec import", "Import fehlgeschlagen", "Importação falhou", "Импорт не удался", "فشل الاستيراد")),
    )


def build_extend():
    from copy import deepcopy
    ext = {
        "nav": build_nav(),
        "footer": build_footer(),
        "leaderboard": build_leaderboard(),
        "home": build_home(),
        "settings": build_settings(),
        "ui": build_ui(),
        "errors": build_errors(),
        "walletConnect": build_wallet_connect(),
        "siwe": build_siwe(),
        "oneTimeSecret": build_one_time_secret(),
        "connect": build_connect(),
        "importPage": build_import_page(),
    }
    return ext

EXTEND = build_extend()
