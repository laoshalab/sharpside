// lib/siwe.js · EIP-6963 多钱包发现 + EIP-4361 SIWE 消息构造。
// 对应钱包登录方案（模型 A · 身份钱包）。纯前端，零依赖。
//
// EIP-6963：已注入钱包通过 `eip6963:announceProvider` 事件公告自身
//   { info: { uuid, name, icon, rdns }, provider: EIP1193Provider }
// dApp 监听并主动 dispatch `eip6963:requestProvider` 请求重新公告，
// 解决"监听器晚于钱包公告注册"的竞态。多个钱包各占一个 provider，
// 用户可在 MetaMask / TP / Rabby / OKX 等已装钱包中任选其一。

import { t } from '../i18n/index.js';

const ANNOUNCE = 'eip6963:announceProvider';
const REQUEST = 'eip6963:requestProvider';

/// 发现所有已注入钱包（EIP-6963）。返回 `[{ info, provider }]`，按 rdns 去重。
///
/// 无 EIP-6963 公告但存在 `window.ethereum` 时，回退补一个兜底项
/// （兼容旧钱包/未实现 EIP-6963 的注入）。
export function discoverWallets(timeoutMs = 300) {
  return new Promise((resolve) => {
    const found = new Map(); // rdns|uuid|name -> { info, provider }
    const handler = (e) => {
      const { info, provider } = (e && e.detail) || {};
      if (!info || !provider) return;
      const key = info.rdns || info.uuid || info.name;
      if (!found.has(key)) found.set(key, { info, provider });
    };
    window.addEventListener(ANNOUNCE, handler);
    // 主动请求已注入钱包重新公告（竞态修复）
    window.dispatchEvent(new Event(REQUEST));
    setTimeout(() => {
      window.removeEventListener(ANNOUNCE, handler);
      if (found.size === 0 && window.ethereum) {
        found.set('window.ethereum', {
          info: { name: t('siwe.injectedWallet'), rdns: 'window.ethereum', icon: null },
          provider: window.ethereum,
        });
      }
      resolve([...found.values()]);
    }, timeoutMs);
  });
}

/// 请求账户（EIP-1193 `eth_requestAccounts`）。返回地址数组。
export async function connect(wallet) {
  return wallet.provider.request({ method: 'eth_requestAccounts' });
}

/// EIP-191 `personal_sign`。返回 `0x` hex 签名（65 字节 r||s||v）。
export async function personalSign(wallet, message, address) {
  return wallet.provider.request({ method: 'personal_sign', params: [message, address] });
}

/// 规范化 RFC3339：截断到毫秒（signinwithethereum / time 对纳秒小数位更挑剔）。
function rfc3339Ms(iso) {
  if (!iso) return iso;
  // 2026-07-22T04:29:13.495445654Z → 2026-07-22T04:29:13.495Z
  const m = String(iso).match(
    /^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2})(?:\.(\d+))?(Z|[+-]\d{2}:\d{2})?$/,
  );
  if (!m) return iso;
  const frac = (m[2] || '').padEnd(3, '0').slice(0, 3);
  const tz = m[3] || 'Z';
  return `${m[1]}.${frac}${tz}`;
}

/// 构造 EIP-4361 SIWE 消息文本（与后端 `signinwithethereum` 解析器格式一致）。
///
/// 注意：消息**不能**以多余空行结尾，否则解析报 Unexpected Content。
/// `domain` 须用服务端 `/auth/wallet/nonce` 返回的 domain（保证与 PUBLIC_DOMAIN 一致）。
export function buildSiwe({
  domain,
  address,
  uri,
  chainId,
  nonce,
  issuedAt,
  expirationTime,
  statement = 'Connect wallet to Sharpside',
}) {
  const lines = [
    `${domain} wants you to sign in with your Ethereum account:`,
    address,
    '',
    statement,
    '',
    `URI: ${uri}`,
    'Version: 1',
    `Chain ID: ${chainId}`,
    `Nonce: ${nonce}`,
    `Issued At: ${rfc3339Ms(issuedAt)}`,
  ];
  if (expirationTime) lines.push(`Expiration Time: ${rfc3339Ms(expirationTime)}`);
  // 故意不用尾部换行：split('\n') 会多出空行 → Unexpected Content
  return lines.join('\n');
}
