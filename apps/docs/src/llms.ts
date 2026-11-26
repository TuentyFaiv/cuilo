import { getCollection } from "astro:content";

import packs from "./data/packs.json" with { type: "json" };

/** The site the links in llms.txt point to. */
export const SITE = "https://cuilo.dev";

/** Sections in the order the sidebar shows them; pages that match none come last. */
const ORDER = [
	"introduction",
	"install",
	"first-piece",
	"guides/",
	"reference/",
	"packs/",
	"agents/",
	"security",
	"migrating-from-v1",
	"changelog",
];

/** Lines that only import a component or open, close or place one, which mean nothing as plain text. */
const COMPONENT_LINE = /^\s*(import\s.+\sfrom\s|<\/?[A-Z][\w.]*(\s[^>]*)?\/?>\s*$)/;

/** Every English docs page except the landing, in sidebar order. */
export async function englishPages() {
	const entries = await getCollection("docs", (entry) => entry.id !== "index" && !entry.id.startsWith("es-mx/"));
	const rank = (id: string) => {
		const index = ORDER.findIndex((prefix) => id === prefix || (prefix.endsWith("/") && id.startsWith(prefix)));
		return index === -1 ? ORDER.length : index;
	};
	return entries.sort((a, b) => rank(a.id) - rank(b.id) || a.id.localeCompare(b.id));
}

/** The public URL of a docs page. */
export function pageUrl(id: string): string {
	return `${SITE}/${id}/`;
}

/** A page's Markdown without import lines and self-closing components. */
export function plainBody(body: string | undefined): string {
	return (body ?? "")
		.split("\n")
		.filter((line) => !COMPONENT_LINE.test(line))
		.join("\n")
		.replace(/<!--[\s\S]*?-->/g, "")
		.trim();
}

/** The official packs as plain Markdown: pieces, inputs and stacks. */
export function packsText(): string {
	const sections = packs.map((pack) => {
		const pieces = pack.pieces.map((piece) => {
			const inputs = piece.inputs.map((input) => `  - \`${input.name}\` (${input.type}): ${input.description ?? ""}`);
			return [
				`### ${pack.name}/${piece.name}`,
				"",
				piece.description ?? "",
				"",
				`Stacks: ${piece.variants.join(", ") || "any"}`,
				"",
				"Inputs:",
				...inputs,
			].join("\n");
		});
		return [`## ${pack.name} ${pack.version}`, "", pack.description ?? "", "", ...pieces].join("\n");
	});
	return ["# Official packs", "", ...sections].join("\n\n");
}
