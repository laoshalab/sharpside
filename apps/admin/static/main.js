// main.js · admin 前端入口。注册路由并启动。对应 docs/FRONTEND_DESIGN.md §7 + P1。
import { route, startRouter } from './router.js';
import { loginPage } from './pages/login.js';
import { mappingsPage } from './pages/mappings.js';
import { identitiesPage } from './pages/identities.js';
import { hotWalletsPage } from './pages/hot-wallets.js';
import { tagRulesPage } from './pages/tag-rules.js';
import { visibilityPage } from './pages/visibility.js';
import { auditThresholdsPage } from './pages/audit-thresholds.js';
import { categoryMappingPage } from './pages/category-mapping.js';
import { shadowHealthPage } from './pages/shadow-health.js';

route('/login', loginPage, 'guest');
route('/', mappingsPage, 'auth');
route('/mappings', mappingsPage, 'auth');
route('/identities', identitiesPage, 'auth');
route('/hot-wallets', hotWalletsPage, 'auth');
route('/tag-rules', tagRulesPage, 'auth');
route('/visibility', visibilityPage, 'auth');
route('/traders', visibilityPage, 'auth');
route('/category-mapping', categoryMappingPage, 'auth');
route('/audit-thresholds', auditThresholdsPage, 'auth');
route('/shadow-health', shadowHealthPage, 'auth');

startRouter();
