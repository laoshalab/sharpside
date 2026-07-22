// pages/connect.js · 兼容旧路径 #/connect、#/login：留在当前页/首页并直接弹窗。
import { navigate } from '../router.js';
import { connectWalletFlow } from '../lib/wallet-connect.js';
import { toast } from '../store/toast.js';
import { t } from '../i18n/index.js';

export async function connectPage() {
  // 不渲染独立页面：回到首页并立即弹出钱包选择器
  navigate('/');
  // 等首页渲染一帧后再弹，避免遮罩被路由清空
  queueMicrotask(async () => {
    try {
      await connectWalletFlow();
    } catch (e) {
      toast(e.message || t('common.connectFailed'), 'error');
    }
  });
  return document.createDocumentFragment();
}
