# YAML-driven app prototype

React, Vite and TypeScript with Ant Design components and Tailwind utilities.
Requires Node 22.12+ and npm.

```sh
# From the repository root
npm ci --prefix apps/app-shell
npm run dev --prefix apps/app-shell
```

Open http://127.0.0.1:5186. The server listens only on loopback and fails if the
port is occupied. Edit **src/config/app.yaml**; Vite applies changes during
development. For a production preview, run `npm run build --prefix apps/app-shell`,
stop the dev server, then run `npm run preview --prefix apps/app-shell`.
Production changes require rebuilding.

## Configuration

The example YAML is authoritative; there is no settings editor. Try:

- Change `branding.name`, `branding.mark` and `theme.primary` (quote hex colors).
- Rename a navigation label or change its `content` from `welcome` to `notes`.
- Move navigation into `layout.header: [brand, navigation]` and set
  `layout.sidebar.regions: []` to remove the sidebar.
- Reorder `layout.content` to `[configuration, page]`, or use `[page]` alone.
- Set `layout.footer: []` to hide the footer.

`branding.name` and at least one navigation entry are required. Other fields
have defaults in `src/config/schema.ts`. Colors accept six-digit hex values;
choose readable color combinations. Sidebar width accepts 180–320 pixels,
radius 0–24. Navigation IDs are unique lowercase route names, used as hash URLs.
An unknown or empty route displays the first configured page.

Registered regions are intentionally limited:

| Region | Allowed IDs |
| --- | --- |
| Header, sidebar | `brand`, `navigation` |
| Content | `page`, `configuration` |
| Footer | `footer` |

Navigation must appear exactly once. Content must include `page`, which renders
the selected entry's registered `welcome` or `notes` React component from
`src/content.tsx`. An empty sidebar/header/footer list omits that region.
When sidebar collapse is enabled, a header toggle remains available even if
header regions are empty. Below 768px the sidebar stacks above content and
starts collapsed; if collapse is disabled, it stays visible above content.
No code, HTML, URLs or arbitrary component imports are executed from YAML.
Unknown fields, components, tags, aliases and duplicate keys are rejected.
Errors display the configuration path (or YAML line/column for syntax errors)
instead of partially rendering a shell. Notes are temporary page state.

## Styles and checks

The Vite Tailwind plugin supplies utilities for spacing, grids and responsive
layout. Ant Design supplies Layout, Menu, Card, Input, Button and theme tokens.
`StyleProvider layer` wraps `ConfigProvider`, with CSS layer order
`theme, base, antd, components, utilities`. This follows
[Ant Design's compatibility guidance](https://ant.design/docs/react/compatible-style/)
and [Tailwind's Vite setup](https://tailwindcss.com/docs/installation/using-vite).
No additional unlayered Ant Design reset is imported. Modern browsers required.

```sh
npm run typecheck --prefix apps/app-shell
npm test --prefix apps/app-shell
npm run build --prefix apps/app-shell
```

Tests cover YAML defaults and rejection, region placement/order, configuration
errors, navigation and registered content interactions. This is a local
prototype with one bundled YAML file, without backend or remote loading.
