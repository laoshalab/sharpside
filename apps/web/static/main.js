// main.js · 前端入口。注册路由并启动。对应 docs/FRONTEND_DESIGN.md §8。
import { route, startRouter, navigate, remount } from './router.js';
import { initI18n, onLocaleChange } from './i18n/index.js';
import { initTheme } from './store/theme.js';
import { homePage } from './pages/home.js';
import { connectPage } from './pages/connect.js';
import { leaderboardPage } from './pages/leaderboard.js';
import { traderPage } from './pages/trader.js';
import { followsPage, newFollowPage } from './pages/follows.js';
import { watchlistPage } from './pages/watchlist.js';
import { dashboardPage } from './pages/dashboard.js';
import { portfolioPage } from './pages/portfolio.js';
import { delegationPage } from './pages/delegation.js';
import { credentialsPage } from './pages/credentials.js';
import { walletPage } from './pages/wallet.js';
import { settingsPage } from './pages/settings.js';
import { subscriptionPage } from './pages/subscription.js';
import { daemonKeyPage } from './pages/daemon-key.js';
import { importPage } from './pages/import.js';

initTheme();
initI18n();
onLocaleChange(() => { remount(); });

// 公开页
route('/', homePage);
route('/connect', connectPage, 'guest');
// 兼容旧书签 #/login → #/connect
route('/login', async () => { navigate('/connect'); }, 'guest');
route('/leaderboard', leaderboardPage);
route('/traders/:platform/:address', traderPage);
// 兼容旧书签 #/import → #/watchlist（导入已并入观察名单页）
route('/import', importPage);

// 鉴权页
route('/dashboard', dashboardPage, 'auth');
route('/portfolio', portfolioPage, 'auth');
route('/wallet', walletPage, 'auth');
route('/follows', followsPage, 'auth');
route('/follows/new', newFollowPage, 'auth');
route('/watchlist', watchlistPage, 'auth');
// 成交历史已并入 #/follows 下方；保留旧书签跳转
route('/copy-history', async () => { navigate('/follows'); }, 'auth');
route('/settings', settingsPage, 'auth');
route('/settings/subscription', subscriptionPage, 'auth');
route('/settings/daemon-key', daemonKeyPage, 'auth');
route('/settings/delegation', delegationPage, 'auth');
route('/settings/credentials', credentialsPage, 'auth');

startRouter();
