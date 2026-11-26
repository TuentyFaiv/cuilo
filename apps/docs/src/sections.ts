import type { UiStrings } from "./i18n/ui";

import { localized, ui } from "./i18n/ui";

/** A part of the docs with its own header tab, sidebar groups and Previous/Next links. */
export interface Section {
	/** The `ui` string that labels its tab. */
	label: keyof UiStrings;
	/** Where its tab links to, without the locale prefix. */
	link: string;
	/** Prefixes of its pages' paths, without the locale prefix. */
	prefixes: string[];
	/** Whether its pages are full width, without a sidebar or a table of contents. */
	fullWidth?: boolean;
}

/** A section's header tab on a given page. */
export interface SectionTab {
	label: string;
	href: string;
	/** Whether the page is in this section. */
	current: boolean;
}

/** The sections, in the order the header shows them. */
export const SECTIONS: readonly Section[] = [
	{
		label: "navDocs",
		link: "/introduction/",
		prefixes: [
			"/introduction/",
			"/install/",
			"/first-piece/",
			"/guides/",
			"/packs/",
			"/security/",
			"/migrating-from-v1/",
		],
	},
	{ label: "navDirectory", link: "/directory/", prefixes: ["/directory/"], fullWidth: true },
	{ label: "navReference", link: "/reference/cli/", prefixes: ["/reference/"] },
	{ label: "navAgents", link: "/agents/mcp/", prefixes: ["/agents/"] },
	{ label: "navChangelog", link: "/changelog/", prefixes: ["/changelog/"], fullWidth: true },
];

/** `pathname` without the locale prefix, e.g. `/install/` for `/es-mx/install/`. */
export function unlocalized(pathname: string, locale: string | undefined): string {
	const prefix = locale ? `/${locale}` : "";
	return prefix && pathname.startsWith(`${prefix}/`) ? pathname.slice(prefix.length) : pathname;
}

/** The section a page or link belongs to; `undefined` for the landing and 404 pages. */
export function sectionOf(pathname: string, locale: string | undefined): Section | undefined {
	const path = unlocalized(pathname, locale);
	return SECTIONS.find((section) => section.prefixes.some((prefix) => path.startsWith(prefix)));
}

/** Every section's tab on the page at `pathname`. */
export function sectionTabs(pathname: string, locale: string | undefined): SectionTab[] {
	const t = ui(locale);
	const current = sectionOf(pathname, locale);
	return SECTIONS.map((section) => ({
		label: t[section.label],
		href: localized(locale, section.link),
		current: section === current,
	}));
}

/** The full-width section a page sits below, such as the directory for `/directory/ui/`; its breadcrumb parent. */
export function parentSection(pathname: string, locale: string | undefined): Section | undefined {
	const section = sectionOf(pathname, locale);
	return section?.fullWidth && unlocalized(pathname, locale) !== section.link ? section : undefined;
}
