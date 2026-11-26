import type { APIRoute } from "astro";

import { englishPages, pageUrl, SITE } from "../llms";

/** An index of every English docs page for AI agents, following llmstxt.org. */
export const GET: APIRoute = async () => {
	const pages = await englishPages();
	const lines = [
		"# cuilo",
		"",
		"> cuilo generates components, resources and middleware from versioned packs, shows every change as a plan before writing, and merges pack updates into the code you edited.",
		"",
		`The full text of every page is at ${SITE}/llms-full.txt.`,
		"",
		"## Docs",
		"",
		...pages.map((page) => `- [${page.data.title}](${pageUrl(page.id)}): ${page.data.description ?? ""}`),
		"",
	];
	return new Response(lines.join("\n"), { headers: { "Content-Type": "text/plain; charset=utf-8" } });
};
