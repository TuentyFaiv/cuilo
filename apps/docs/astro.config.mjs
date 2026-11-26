import starlight from "@astrojs/starlight";
import { defineConfig } from "astro/config";

const spanish = "es-MX";

/** A sidebar link to `slug`, with its label in both languages. */
const page = (slug, label, translation) => ({ slug, label, translations: { [spanish]: translation } });

export default defineConfig({
	site: "https://cuilo.dev",
	trailingSlash: "always",
	integrations: [
		starlight({
			title: "cuilo",
			description:
				"Generate components, resources and middleware from versioned packs, see every file before it lands, and keep your edits when packs update.",
			favicon: "/favicon.svg",
			defaultLocale: "root",
			locales: {
				root: { label: "English", lang: "en" },
				"es-mx": { label: "Español", lang: spanish },
			},
			social: [{ icon: "github", label: "GitHub", href: "https://github.com/TuentyFaiv/cuilo" }],
			editLink: { baseUrl: "https://github.com/TuentyFaiv/cuilo/edit/main/apps/docs/" },
			credits: false,
			customCss: [
				"@fontsource-variable/inter",
				"@fontsource-variable/bricolage-grotesque/opsz.css",
				"@fontsource/fragment-mono",
				"./src/styles/theme.css",
			],
			expressiveCode: {
				themes: ["github-dark"],
				useStarlightDarkModeSwitch: false,
				useStarlightUiThemeColors: false,
				styleOverrides: {
					borderColor: "var(--cu-code-line)",
					borderRadius: "0.625rem",
					codeBackground: "var(--cu-code-bg)",
					codeFontFamily: "'Fragment Mono', ui-monospace, monospace",
					codeFontSize: "0.875rem",
					codeLineHeight: "1.7",
					uiFontFamily: "'Inter Variable', system-ui, sans-serif",
					frames: {
						editorActiveTabBackground: "var(--cu-code-bg)",
						editorActiveTabIndicatorBottomColor: "var(--cu-accent-2)",
						editorTabBarBackground: "var(--cu-code-bg)",
						editorTabBarBorderBottomColor: "var(--cu-code-line)",
						frameBoxShadowCssValue: "none",
						terminalBackground: "var(--cu-code-bg)",
						terminalTitlebarBackground: "var(--cu-code-bg)",
						terminalTitlebarBorderBottomColor: "var(--cu-code-line)",
						terminalTitlebarDotsOpacity: "0.35",
					},
				},
			},
			components: {
				// biome-ignore lint/style/useNamingConvention: Starlight component override key
				Footer: "./src/components/footer.astro",
				// biome-ignore lint/style/useNamingConvention: Starlight component override key
				Header: "./src/components/header.astro",
				// biome-ignore lint/style/useNamingConvention: Starlight component override key
				Hero: "./src/components/hero.astro",
				// biome-ignore lint/style/useNamingConvention: Starlight component override key
				PageTitle: "./src/components/page-title.astro",
				// biome-ignore lint/style/useNamingConvention: Starlight component override key
				Sidebar: "./src/components/sidebar.astro",
				// biome-ignore lint/style/useNamingConvention: Starlight component override key
				SiteTitle: "./src/components/site-title.astro",
			},
			routeMiddleware: "./src/route-data.ts",
			// Each page shows only the groups of its section in src/sections.ts.
			sidebar: [
				{
					label: "Start here",
					translations: { [spanish]: "Empieza aquí" },
					items: ["introduction", "install", "first-piece"],
				},
				{
					label: "Guides",
					translations: { [spanish]: "Guías" },
					items: [
						"guides/packs-and-pieces",
						"guides/stack-detection",
						"guides/updates",
						"guides/verify",
						"guides/history",
						"guides/formatters-and-trust",
						"guides/dependencies",
						"guides/adopt-and-capture",
					],
				},
				{
					label: "Build packs",
					translations: { [spanish]: "Crear packs" },
					items: ["packs/write-a-pack", "packs/sources", "packs/signing", "packs/registry-items"],
				},
				{
					label: "More",
					translations: { [spanish]: "Más" },
					items: ["security", "migrating-from-v1"],
				},
				{ label: "CLI", items: [{ autogenerate: { directory: "reference/cli" } }] },
				{
					label: "Files",
					translations: { [spanish]: "Archivos" },
					items: [
						page("reference/configuration", "Configuration", "Configuración"),
						page("reference/user-config", "User config", "Configuración de usuario"),
						page("reference/lockfile", "Lockfile", "Lockfile"),
						page("reference/pack-manifest", "Pack manifest", "Manifiesto del pack"),
						page("reference/piece-manifest", "Piece manifest", "Manifiesto de la pieza"),
					],
				},
				{
					label: "Runtime",
					translations: { [spanish]: "Ejecución" },
					items: ["reference/environment", "reference/exit-codes"],
				},
				{
					label: "Agents",
					translations: { [spanish]: "Agentes" },
					items: ["agents/mcp", "agents/llms-txt"],
				},
			],
		}),
	],
});
