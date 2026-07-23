// lib/clipboard.js · 安全复制：失败不假成功。
import { toast } from '../store/toast.js';
import { t } from '../i18n/index.js';

/// 复制文本到剪贴板；成功/失败均 toast。返回是否成功。
export async function copyText(text, { successMsg, errorMsg } = {}) {
  const value = String(text ?? '');
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(value);
    } else {
      // 非安全上下文 fallback
      const ta = document.createElement('textarea');
      ta.value = value;
      ta.setAttribute('readonly', '');
      ta.style.position = 'fixed';
      ta.style.left = '-9999px';
      document.body.appendChild(ta);
      ta.select();
      const ok = document.execCommand('copy');
      ta.remove();
      if (!ok) throw new Error('execCommand failed');
    }
    toast(successMsg || t('common.copied'), 'success');
    return true;
  } catch {
    toast(errorMsg || t('common.copyFailed'), 'error');
    return false;
  }
}
