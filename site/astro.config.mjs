// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

const repo = 'https://github.com/roxguel/godot-dev-tools';

export default defineConfig({
  site: 'https://gdt.roxguel.dev',
  integrations: [
    starlight({
      title: 'Godot Development Tools',
      description:
        'gdt: one cross-platform command to manage Godot versions, addons and tests. An independent community project.',
      favicon: '/favicon.svg',
      social: [{ icon: 'github', label: 'GitHub', href: repo }],
      // Hand-written pages edit under site/; generated ones set their own editUrl.
      editLink: { baseUrl: `${repo}/edit/main/site/` },
      // Search is Pagefind, built into Starlight: local, no third-party service.
      customCss: ['./src/styles/custom.css'],
      components: { Footer: './src/components/Footer.astro' },
      sidebar: [
        {
          label: 'Installation',
          items: [
            { slug: 'installation' },
            { label: 'Read the installers', link: '/install/' },
            { slug: 'installation/from-source' },
            { slug: 'installation/verify' },
          ],
        },
        { slug: 'quick-start' },
        { label: 'Guides', items: [{ autogenerate: { directory: 'guides' } }] },
        { label: 'Command reference', items: [{ slug: 'reference/cli' }] },
        { label: 'Specifications', collapsed: true, items: [{ autogenerate: { directory: 'specs' } }] },
        { label: 'Architecture decisions', collapsed: true, items: [{ autogenerate: { directory: 'adr' } }] },
        { label: 'Project', collapsed: true, items: [{ autogenerate: { directory: 'project' } }] },
        { slug: 'security' },
        { slug: 'changelog' },
      ],
    }),
  ],
});
