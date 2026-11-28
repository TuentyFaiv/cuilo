import { stackName } from "./i18n/ui";

/**
 * Variants in the order the site lists their stacks and picks examples. cuilo gives first support to Svelte, Astro,
 * the Rust stacks (Leptos, Dioxus, Yew and Axum) and SwiftUI, so they lead. The other UI stacks follow in the order the
 * ui pack declares them, then the other backend stacks in the backend pack's order, each stack's ORM and major variants
 * after it.
 */
export const STACK_PRIORITY = [
	"svelte",
	"astro",
	"leptos",
	"dioxus",
	"yew",
	"axum",
	"swiftui",
	"swiftui-package",
	"angular",
	"qwik",
	"solid",
	"vue",
	"react",
	"lit",
	"flutter",
	"compose-multiplatform",
	"compose",
	"hono",
	"hono-drizzle",
	"hono-prisma",
	"express",
	"express-drizzle",
	"express-prisma",
	"fastapi",
	"chi",
	"echo",
	"echo-v5",
] as const;

const variantRanks = new Map<string, number>(STACK_PRIORITY.map((variant, index) => [variant, index]));
const stackRanks = new Map<string, number>();
for (const [index, variant] of STACK_PRIORITY.entries()) {
	const name = stackName(variant);
	if (!stackRanks.has(name)) {
		stackRanks.set(name, index);
	}
}

const rank = (ranks: Map<string, number>, key: string) => ranks.get(key) ?? STACK_PRIORITY.length;

/** Compares two variants, e.g. `svelte` and `react`, by priority; variants the list leaves out tie, after the rest. */
export const compareVariants = (a: string, b: string): number => rank(variantRanks, a) - rank(variantRanks, b);

/** Compares two stacks' display names, e.g. `Svelte` and `React`, the way `compareVariants` compares their variants. */
export const compareStacks = (a: string, b: string): number => rank(stackRanks, a) - rank(stackRanks, b);
