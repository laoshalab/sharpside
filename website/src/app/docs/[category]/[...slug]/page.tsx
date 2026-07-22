import { notFound } from "next/navigation";
import { MDXRemote } from "next-mdx-remote/rsc";
import remarkGfm from "remark-gfm";
import rehypeSlug from "rehype-slug";
import rehypeAutolinkHeadings from "rehype-autolink-headings";
import { getLocale } from "next-intl/server";
import {
  getDocBySlug,
  getAdjacentDocs,
  getAllDocParams,
  isDocCategory,
} from "@/lib/docs";
import { DocArticleShell } from "@/components/docs/DocArticleShell";
import { mdxComponents } from "@/components/docs/mdx";

interface PageProps {
  params: {
    category: string;
    slug: string[];
  };
}

export function generateStaticParams() {
  return getAllDocParams("zh");
}

export default async function DocPage({ params }: PageProps) {
  const { category, slug } = params;
  const locale = await getLocale();

  if (!isDocCategory(category)) {
    notFound();
  }

  const doc = getDocBySlug(locale, category, slug);
  if (!doc) notFound();

  const adjacent = getAdjacentDocs(locale, category, slug.join("/"));

  return (
    <DocArticleShell doc={doc} category={category} prev={adjacent.prev} next={adjacent.next}>
      <MDXRemote
        source={doc.content}
        components={mdxComponents}
        options={{
          mdxOptions: {
            remarkPlugins: [remarkGfm],
            rehypePlugins: [rehypeSlug, [rehypeAutolinkHeadings, { behavior: "wrap" }]],
          },
        }}
      />
    </DocArticleShell>
  );
}
