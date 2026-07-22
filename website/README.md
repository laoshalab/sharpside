# Sharpside Website

Sharpside 产品与技术知识库（Next.js App Router + MDX），结构对齐 Nexus 网站的 `/docs` 体系。

## 开发

```bash
cd website
npm install
npm run dev
```

- 首页：http://localhost:3000
- 知识库：http://localhost:3000/docs

## 多语言

支持 5 种语言（cookie `locale`，非 URL 前缀）：

| 代码 | 语言 |
|------|------|
| `zh` | 中文（默认） |
| `en` | English |
| `ja` | 日本語 |
| `ko` | 한국어 |
| `es` | Español |

- UI 文案：`messages/{locale}.json`
- 语言配置：`src/i18n/config.ts`
- 导航栏右上角可切换语言

MDX 正文目前以 `content/zh/` 为主；其他语言暂回退到中文内容，可按同路径在 `content/{locale}/` 补充翻译。

## 内容

MDX 位于 `content/{locale}/{guide|technical}/**/*.mdx`。

- **guide**：扁平文章，按 frontmatter `order` 排序
- **technical**：按一级目录分组（`getting-started` / `architecture` / `operations`）

新增文章只需添加 `.mdx` 并填写 frontmatter，侧栏与静态路由会自动更新。

## 结构对照（Nexus → Sharpside）

| Nexus | Sharpside |
|-------|-----------|
| `business` / `technical` | `guide` / `technical` |
| 三栏 DocsShell | 同左 |
| `src/lib/docs.ts` 扫文件系统 | 同左 |
| Callout / MetricCard / CompareTable | 同左 |
| next-intl（10 语） | next-intl（5 语） |
| 品牌色 cyan/purple | 青绿 `#00C2A8` + 琥珀 `#FFB020` |
