// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

const site = 'https://taulit.github.io';
const base = '/gpui-markdown-editor';
const ogImage = `${site}${base}/og.png`;

// https://starlight.astro.build/reference/configuration/
export default defineConfig({
  site,
  base,
  integrations: [
    starlight({
      title: 'gpui-markdown-editor',
      description:
        'WYSIWYG Markdown editor component for GPUI, the Rust UI framework of Zed: lossless round-trip, input rules, tables, checklists and images.',
      logo: { src: './src/assets/logo.svg', alt: 'gpui-markdown-editor' },
      favicon: '/favicon.svg',
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/taulit/gpui-markdown-editor' },
      ],
      editLink: {
        baseUrl: 'https://github.com/taulit/gpui-markdown-editor/edit/main/site/',
      },
      customCss: ['./src/styles/custom.css'],
      head: [
        { tag: 'meta', attrs: { property: 'og:image', content: ogImage } },
        { tag: 'meta', attrs: { property: 'og:image:width', content: '1200' } },
        { tag: 'meta', attrs: { property: 'og:image:height', content: '630' } },
        {
          tag: 'meta',
          attrs: {
            property: 'og:image:alt',
            content: 'gpui-markdown-editor: a note with a heading, checklist and table, edited as formatted text',
          },
        },
        { tag: 'meta', attrs: { name: 'twitter:card', content: 'summary_large_image' } },
        { tag: 'meta', attrs: { name: 'twitter:image', content: ogImage } },
      ],
      sidebar: [
        {
          label: 'Related projects',
          items: [
            {
              label: 'Sheaf – notes for GNOME',
              link: 'https://taulit.github.io/sheaf/',
              attrs: { target: '_blank', rel: 'noopener' },
            },
          ],
        },
        {
          label: 'Guide',
          items: [
            { label: 'Quickstart', slug: 'quickstart' },
            { label: 'Features', slug: 'features' },
            { label: 'API overview', slug: 'api' },
            { label: 'Theming', slug: 'theming' },
            { label: 'Benchmarks', slug: 'benchmarks' },
            { label: 'FAQ', slug: 'faq' },
          ],
        },
        {
          label: 'Reference',
          items: [
            { label: 'API docs on docs.rs', link: 'https://docs.rs/gpui-markdown-editor' },
            { label: 'Changelog', link: 'https://github.com/taulit/gpui-markdown-editor/blob/main/CHANGELOG.md' },
          ],
        },
      ],
    }),
  ],
});
