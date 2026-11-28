import { defineRouteMiddleware } from "@astrojs/starlight/route-data";

import type { StarlightRouteData } from "@astrojs/starlight/route-data";
import type { Section } from "./sections";

import { sectionOf } from "./sections";

type SidebarEntry = StarlightRouteData["sidebar"][number];
type Pagination = StarlightRouteData["pagination"];
type SidebarLink = NonNullable<Pagination["prev"]>;

/** The entries of `sidebar` that link into `section`, leaving out groups that end up empty. */
function within(sidebar: SidebarEntry[], section: Section, locale: string | undefined): SidebarEntry[] {
	return sidebar.flatMap((entry): SidebarEntry[] => {
		if (entry.type === "link") {
			return sectionOf(entry.href, locale) === section ? [entry] : [];
		}
		const entries = within(entry.entries, section, locale);
		return entries.length > 0 ? [{ ...entry, entries }] : [];
	});
}

/** Every link of `sidebar`, in order. */
function links(sidebar: SidebarEntry[]): SidebarLink[] {
	return sidebar.flatMap((entry) => (entry.type === "link" ? [entry] : links(entry.entries)));
}

/** The links before and after the current page in the route's sidebar. A page's own `prev` or `next` frontmatter keeps Starlight's link. */
function neighbours(route: StarlightRouteData): Pagination {
	const all = links(route.sidebar);
	const index = all.findIndex((link) => link.isCurrent);
	const near = (offset: number) => (index === -1 ? undefined : all[index + offset]);
	const { prev, next } = route.entry.data;
	return {
		prev: prev === undefined ? near(-1) : route.pagination.prev,
		next: next === undefined ? near(1) : route.pagination.next,
	};
}

/** Keeps each page's sidebar and Previous/Next links inside its section; full-width sections get no sidebar or table of contents. */
export const onRequest = defineRouteMiddleware((context) => {
	const route = context.locals.starlightRoute;
	const section = sectionOf(context.url.pathname, route.locale);
	if (!section) {
		return;
	}
	route.sidebar = within(route.sidebar, section, route.locale);
	route.pagination = neighbours(route);
	if (section.fullWidth) {
		route.hasSidebar = false;
		route.toc = undefined;
	}
});
