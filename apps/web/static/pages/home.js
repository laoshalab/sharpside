// pages/home.js · 首页 / Venue 总览。对应 docs/FRONTEND_DESIGN.md §6.7（P0–P2）。
import { el, emptyState, skeleton, traderLabel, platformIcon, pnlClass, fmtPct } from '../components/ui.js';
import { withShell } from '../components/nav.js';
import { listVenues, listTraders } from '../lib/venue-hub.js';
import { getUser, isLoggedIn } from '../store/auth.js';
import { navigate } from '../router.js';
import { connectWalletFlow } from '../lib/wallet-connect.js';
import { toast } from '../store/toast.js';
import { t } from '../i18n/index.js';

function isProUser() {
  const u = getUser();
  const tier = String(u?.subscription_tier || u?.tier || 'free').toLowerCase();
  return tier === 'pro_plus' || (tier && tier !== 'free');
}

/** 未接入 Venue 的路线图占位（与 §6.7 / 凭证页 Phase 对齐）。 */
const ROADMAP_VENUES = [
  {
    platform: 'kalshi',
    display_name: 'Kalshi',
    phase: 'Phase 3',
    capabilities: ['execution_venue'],
    auth_model: 'kyc_api_key',
    unit: 'usd_cents',
    geo: 'us_only',
  },
  {
    platform: 'manifold',
    display_name: 'Manifold',
    phase: 'Phase 2',
    capabilities: ['signal_source'],
    auth_model: 'api_key',
    unit: 'mana',
    geo: 'global',
  },
];

const AUTH_LABEL = { wallet: 'Wallet', kyc_api_key: 'KYC', api_key: 'API key' };
const UNIT_LABEL = { usdc_ctf: 'USDC', usd_cents: 'USD cents', mana: 'Mana', native: 'Native' };
const GEO_LABEL = {
  global: 'Global',
  us_only: 'US only',
};

function capLabel(caps = []) {
  const CAP_LABEL = {
    signal_source: t('home.venue.capSignal'),
    execution_venue: t('home.venue.capExecution'),
  };
  const parts = caps.map(c => CAP_LABEL[c] || c).filter(Boolean);
  return parts.length ? parts.join(' + ') : '—';
}

function metaLine(v) {
  const auth = v.auth_model === 'none'
    ? t('home.venue.authNone')
    : (AUTH_LABEL[v.auth_model] || v.auth_model || '—');
  const unit = UNIT_LABEL[v.unit] || v.unit || '—';
  return `${auth} · ${unit}`;
}

function geoLabel(geo) {
  if (geo === 'global_with_us_restrictions') return t('home.venue.geoUsRestrict');
  return GEO_LABEL[geo] || geo || '—';
}

function buildHero() {
  const loggedIn = isLoggedIn();
  const secondary = loggedIn
    ? el('button', {
        class: 'ghost home-cta',
        text: t('home.goDashboard'),
        onclick: () => navigate('/dashboard'),
      })
    : el('button', {
        class: 'ghost home-cta',
        text: t('home.connectWallet'),
        onclick: () => connectWalletFlow().catch(() => {}),
      });

  const copy = el('div', { class: 'home-hero-copy' }, [
    el('div', { class: 'home-hero-brand' }, [
      el('span', { class: 'brand-mark', text: '◈' }),
      el('span', { class: 'home-hero-name', text: 'Sharpside' }),
    ]),
    el('h1', { class: 'home-hero-title', text: t('home.title') }),
    el('p', { class: 'home-hero-sub muted', text: t('home.sub') }),
    el('div', { class: 'home-hero-actions' }, [
      el('button', {
        class: 'primary home-cta',
        text: t('home.discover'),
        onclick: () => navigate('/leaderboard'),
      }),
      secondary,
    ]),
  ]);

  return el('section', { class: 'home-hero home-reveal' }, [copy, buildHeroVenueAura()]);
}

/** Hero 右侧：Venue 图标氛围构图（装饰，非交互卡）。 */
const HERO_VENUE_AURA = [
  { platform: 'polymarket', label: 'Polymarket', slot: 'primary' },
  { platform: 'kalshi', label: 'Kalshi', slot: 'a' },
  { platform: 'manifold', label: 'Manifold', slot: 'b' },
  { platform: 'zeitgeist', label: 'Zeitgeist', slot: 'c' },
  { platform: 'azuro', label: 'Azuro', slot: 'd' },
];

function buildHeroVenueAura() {
  const nodes = HERO_VENUE_AURA.map((v, i) =>
    el('div', {
      class: `hero-aura-node hero-aura-node--${v.slot}`,
      style: `--aura-i:${i}`,
      title: v.label,
    }, [
      el('div', { class: 'hero-aura-orb', html: platformIcon(v.platform) }),
      el('span', { class: 'hero-aura-label', text: v.label }),
    ]),
  );

  return el('aside', {
    class: 'home-hero-aura',
    'aria-hidden': 'true',
  }, [
    el('div', { class: 'hero-aura-bg' }, [
      el('span', { class: 'hero-aura-blob hero-aura-blob--1' }),
      el('span', { class: 'hero-aura-blob hero-aura-blob--2' }),
      el('span', { class: 'hero-aura-blob hero-aura-blob--3' }),
      el('span', { class: 'hero-aura-mesh' }),
    ]),
    el('div', { class: 'hero-aura-glow' }),
    el('div', { class: 'hero-aura-ring hero-aura-ring--outer' }),
    el('div', { class: 'hero-aura-ring hero-aura-ring--inner' }),
    ...nodes,
  ]);
}

function venueCard(v, { locked = false, index = 0 } = {}) {
  const name = v.display_name || v.platform || '—';
  const phase = v.phase || t('home.venue.phaseDefault');
  const iconWrap = el('div', { class: 'venue-card-icon', html: platformIcon(v.platform) });
  const head = el('div', { class: 'venue-card-head' }, [
    iconWrap,
    el('div', { class: 'venue-card-title-wrap' }, [
      el('div', { class: 'venue-card-title', text: name }),
      locked
        ? el('span', { class: 'venue-phase', text: phase })
        : el('span', { class: 'venue-live', text: t('home.venue.liveBadge') }),
    ]),
  ]);

  const body = el('div', { class: 'venue-card-body' }, [
    el('div', { class: 'venue-card-caps', text: capLabel(v.capabilities) }),
    el('div', { class: 'muted', text: metaLine(v) }),
    el('div', { class: 'muted', text: geoLabel(v.geo) }),
  ]);

  const style = `--stagger:${index}`;

  if (locked) {
    return el('button', {
      type: 'button',
      class: 'venue-card venue-card--locked home-stagger',
      style,
      'aria-label': t('home.venue.ariaLocked', { name, phase }),
      onclick: () => toast(t('home.venue.toastComingSoon', { name, phase }), 'info'),
    }, [
      head,
      body,
      el('div', { class: 'venue-card-lock' }, [
        el('span', { class: 'venue-lock-icon', text: '🔒' }),
        el('span', { text: t('home.venue.lockOverlay') }),
      ]),
    ]);
  }

  return el('button', {
    type: 'button',
    class: 'venue-card home-stagger',
    style,
    'aria-label': t('home.venue.ariaLive', { name }),
    onclick: () => navigate('/leaderboard?platform=' + encodeURIComponent(v.platform)),
  }, [head, body]);
}

function buildVenueSection(host) {
  const section = el('section', { class: 'home-section home-reveal', style: '--reveal-delay:80ms' }, [
    el('h2', { text: t('home.venue.sectionTitle') }),
    el('p', { class: 'muted home-section-desc', text: t('home.venue.sectionDesc') }),
    host,
  ]);
  return section;
}

function renderVenues(host, venues) {
  host.innerHTML = '';
  const live = Array.isArray(venues) ? venues : [];
  const liveKeys = new Set(live.map(v => String(v.platform || '').toLowerCase()));
  const roadmap = ROADMAP_VENUES.filter(v => !liveKeys.has(v.platform.toLowerCase()));

  if (live.length === 0 && roadmap.length === 0) {
    host.appendChild(emptyState({
      icon: '∅',
      text: t('home.venue.emptyTitle'),
      action: el('span', { class: 'muted', text: t('home.venue.emptyHint') }),
    }));
    return;
  }

  const grid = el('div', { class: 'venue-grid' });
  let i = 0;
  for (const v of live) grid.appendChild(venueCard(v, { index: i++ }));
  for (const v of roadmap) grid.appendChild(venueCard(v, { locked: true, index: i++ }));
  host.appendChild(grid);
}

function channelCard({ tag, title, lead, points, custody, custodyCls, cta, index = 0 }) {
  return el('div', { class: 'channel-card home-stagger', style: `--stagger:${index}` }, [
    el('div', { class: 'channel-card-tag', text: tag }),
    el('h3', { class: 'channel-card-title', text: title }),
    el('p', { class: 'channel-card-lead muted', text: lead }),
    el('ul', { class: 'channel-card-points' }, points.map(pt => el('li', { text: pt }))),
    el('div', { class: 'channel-custody ' + custodyCls, text: custody }),
    cta,
  ]);
}

function buildChannelsSection() {
  const loggedIn = isLoggedIn();
  const pro = loggedIn && isProUser();

  const ctaA = loggedIn
    ? el('button', {
        class: 'primary home-cta',
        text: t('home.channels.ctaSetupFollows'),
        onclick: () => navigate('/follows'),
      })
    : el('button', {
        class: 'primary home-cta',
        text: t('home.channels.ctaConnectStart'),
        onclick: () => connectWalletFlow({ redirect: '/follows' }).catch(() => {}),
      });

  let ctaB;
  if (!loggedIn) {
    ctaB = el('button', {
      class: 'ghost home-cta',
      text: t('home.channels.ctaConnectUpgrade'),
      onclick: () => connectWalletFlow({ redirect: '/settings/subscription' }).catch(() => {}),
    });
  } else if (pro) {
    ctaB = el('button', {
      class: 'ghost home-cta',
      text: t('home.channels.ctaConfigureDaemon'),
      onclick: () => navigate('/settings/daemon-key'),
    });
  } else {
    ctaB = el('button', {
      class: 'ghost home-cta',
      text: t('home.channels.ctaUpgradePro'),
      onclick: () => navigate('/settings/subscription'),
    });
  }

  return el('section', { class: 'home-section home-reveal', style: '--reveal-delay:120ms' }, [
    el('h2', { text: t('home.channels.sectionTitle') }),
    el('p', {
      class: 'muted home-section-desc',
      text: t('home.channels.sectionDesc'),
    }),
    el('div', { class: 'channel-grid' }, [
      channelCard({
        tag: t('home.channels.aTag'),
        title: t('home.channels.aTitle'),
        lead: t('home.channels.aLead'),
        points: [
          t('home.channels.aPoint1'),
          t('home.channels.aPoint2'),
          t('home.channels.aPoint3'),
        ],
        custody: t('home.channels.aCustody'),
        custodyCls: 'warn',
        cta: ctaA,
        index: 0,
      }),
      channelCard({
        tag: t('home.channels.bTag'),
        title: t('home.channels.bTitle'),
        lead: t('home.channels.bLead'),
        points: [
          t('home.channels.bPoint1'),
          t('home.channels.bPoint2'),
          t('home.channels.bPoint3'),
        ],
        custody: t('home.channels.bCustody'),
        custodyCls: 'ok',
        cta: ctaB,
        index: 1,
      }),
    ]),
  ]);
}

function buildClosingSection() {
  const actions = [
    el('button', {
      class: 'primary home-cta',
      text: t('home.closing.ctaDiscover'),
      onclick: () => navigate('/leaderboard'),
    }),
  ];
  if (!isLoggedIn()) {
    actions.push(el('button', {
      class: 'ghost home-cta',
      text: t('home.closing.ctaConnect'),
      onclick: () => connectWalletFlow().catch(() => {}),
    }));
  }

  return el('section', { class: 'home-section home-closing home-reveal', style: '--reveal-delay:200ms' }, [
    el('h2', { class: 'home-closing-title', text: t('home.closing.title') }),
    el('div', { class: 'home-hero-actions' }, actions),
  ]);
}

function buildHotSection(host) {
  const head = el('div', { class: 'home-section-head' }, [
    el('div', {}, [
      el('h2', { text: t('home.hot.sectionTitle') }),
      el('p', { class: 'muted home-section-desc', text: t('home.hot.sectionDesc') }),
    ]),
    el('a', {
      href: '#/leaderboard',
      class: 'home-section-link',
      text: t('home.hot.viewLeaderboard'),
    }),
  ]);
  return el('section', { class: 'home-section home-reveal', style: '--reveal-delay:160ms' }, [head, host]);
}

function renderHot(host, traders) {
  host.innerHTML = '';
  if (!traders || traders.length === 0) {
    host.appendChild(emptyState({
      icon: '∅',
      text: t('home.hot.emptyTitle'),
      action: el('a', { href: '#/leaderboard', text: t('home.hot.emptyAction') }),
    }));
    return;
  }

  const card = el('div', { class: 'card home-hot-card' });
  card.appendChild(el('table', { class: 'home-hot-table' }, [
    el('thead', {}, [el('tr', {}, [
      el('th', { text: t('home.hot.colTrader') }),
      el('th', { text: t('home.hot.colPlatform') }),
      el('th', { text: '30D ROI' }),
      el('th', { text: t('home.hot.colWinRate') }),
    ])]),
    el('tbody', {}, traders.map(row => el('tr', {
      class: 'clickable',
      onclick: () => navigate(`/traders/${encodeURIComponent(row.platform)}/${encodeURIComponent(row.address)}`),
    }, [
      el('td', { text: traderLabel(row) }),
      el('td', { html: platformIcon(row.platform) }),
      el('td', { class: pnlClass(row.roi), text: fmtPct(row.roi) }),
      el('td', { text: fmtPct(row.win_rate, 0) }),
    ]))),
  ]));
  host.appendChild(card);
}

export async function homePage() {
  const c = el('div', { class: 'container home-page' });
  c.appendChild(buildHero());

  const venueHost = el('div', { class: 'home-venue-host' }, [skeleton(2)]);
  c.appendChild(buildVenueSection(venueHost));

  c.appendChild(buildChannelsSection());

  const hotHost = el('div', { class: 'home-hot-host' }, [skeleton(3)]);
  c.appendChild(buildHotSection(hotHost));

  c.appendChild(buildClosingSection());

  try {
    const venues = await listVenues();
    renderVenues(venueHost, venues);
  } catch (e) {
    venueHost.innerHTML = '';
    venueHost.appendChild(emptyState({
      icon: '⚠',
      text: t('home.venue.loadError'),
      action: el('button', {
        class: 'sm',
        text: t('common.retry'),
        onclick: () => location.reload(),
      }),
    }));
    console.warn('[home] venues', e);
  }

  try {
    const hot = await listTraders({ sort: 'roi', sort_desc: true, period: '1m', limit: 5 });
    renderHot(hotHost, hot);
  } catch (e) {
    hotHost.innerHTML = '';
    hotHost.appendChild(emptyState({
      icon: '⚠',
      text: t('home.hot.loadError'),
      action: el('button', {
        class: 'sm',
        text: t('common.retry'),
        onclick: () => location.reload(),
      }),
    }));
    console.warn('[home] traders', e);
  }

  return withShell(c);
}
