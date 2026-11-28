import type { APIRoute } from "astro";

import { englishPages, packsText, pageUrl, plainBody } from "../llms";

/** The full text of every English docs page, then the official packs, for AI agents. */
export const GET: APIRoute = async () => {
	const pages = await englishPages();
	const sections = pages.map((page) =>
		[`# ${page.data.title}`, "", `Source: ${pageUrl(page.id)}`, "", plainBody(page.body)].join("\n")
	);
	const text = [...sections, packsText(), ""].join("\n\n---\n\n");
	return new Response(text, { headers: { "Content-Type": "text/plain; charset=utf-8" } });
};
