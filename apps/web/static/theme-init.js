// theme-init.js · 主题预置（防刷新闪烁）。须在首次绘制前同步执行。
// 对应 index.html（原内联脚本，安全修复 3.2 抽出以兼容 CSP script-src 'self'）。
(function () {
  try {
    var t = localStorage.getItem('sharpside-theme');
    if (t === 'light') {
      document.documentElement.classList.remove('dark');
      document.documentElement.classList.add('light');
    } else if (t === 'dark') {
      document.documentElement.classList.remove('light');
      document.documentElement.classList.add('dark');
    } else if (window.matchMedia('(prefers-color-scheme: light)').matches) {
      document.documentElement.classList.remove('dark');
      document.documentElement.classList.add('light');
    }
  } catch (e) {}
})();
