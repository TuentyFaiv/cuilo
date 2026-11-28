import type { LucideName } from "./components/lucide-icons";

import packs from "./data/packs.json" with { type: "json" };
import { stackName, variantName } from "./i18n/ui";
import { addCommand } from "./pieces";
import { compareStacks, compareVariants } from "./stack-priority";

/** A pack in `src/data/packs.json`, which `cargo xtask docs` generates. */
export type Pack = (typeof packs)[number];
/** One of a pack's pieces. */
export type Piece = Pack["pieces"][number];

/** How far a pack is trusted: shipped inside cuilo, reviewed for the directory, or only signed by its author. */
export const TRUSTS = ["builtIn", "reviewed", "signed"] as const;
export type Trust = (typeof TRUSTS)[number];

/** The icon each trust's badge shows. */
export const TRUST_ICONS: Record<Trust, LucideName> = {
	builtIn: "package",
	reviewed: "badge-check",
	signed: "key-round",
};

/** What a pack is for, as the `kinds` of its `pack.toml` say. */
export const KINDS = ["ui", "backend", "mobile", "desktop"] as const;
export type Kind = (typeof KINDS)[number];

/** Every pack, in the order the generator lists them. */
export const PACKS: readonly Pack[] = packs;

/** The pack named `name`; fails the build when `src/data/packs.json` has none. */
export function packNamed(name: string): Pack {
	const pack = PACKS.find((candidate) => candidate.name === name);
	if (!pack) {
		throw new Error(`no pack named ${name} in src/data/packs.json; run cargo xtask docs`);
	}
	return pack;
}

/** A URL- and attribute-safe key for a stack's display name, e.g. `jetpack-compose`. */
export function stackKey(name: string): string {
	return name.toLowerCase().replaceAll(" ", "-");
}

/** Display names of `variants`' stacks in priority order; two variants can share one, such as both SwiftUI variants. */
export function variantStacks(variants: readonly string[]): string[] {
	return [...new Set([...variants].sort(compareVariants).map((variant) => stackName(variant)))];
}

/** The display names of the stacks a piece supports, in priority order. */
export function pieceStacks(piece: Piece): string[] {
	return variantStacks(piece.variants);
}

/** The display names of the stacks any of a pack's pieces supports, in priority order. */
export function packStacks(pack: Pack): string[] {
	return [...new Set(pack.pieces.flatMap(pieceStacks))].sort(compareStacks);
}

/** The kinds a pack declares, in the order the Kind filter lists them. */
export function packKinds(pack: Pack): Kind[] {
	return KINDS.filter((kind) => pack.kinds.includes(kind));
}

/** A pack, piece or input in `src/data/packs.json`, with its description and that description's translations. */
interface Described {
	description?: string | null;
	translations: Partial<Record<string, { description: string }>>;
}

/** The description of `item` in `locale` (`undefined` for English), or its English one when the pack has no translation. */
export function describe(item: Described, locale: string | undefined): string {
	return (locale && item.translations[locale]?.description) || item.description || "";
}

/** The variant of `piece` that comes first in priority, e.g. `svelte` for a ui piece; empty when it has none. */
function leadVariant(piece: Piece): string {
	return [...piece.variants].sort(compareVariants)[0] ?? "";
}

/** Compares two pieces by their lead variants, so pieces for the stacks with first support come first. */
export const comparePieces = (a: Piece, b: Piece): number => compareVariants(leadVariant(a), leadVariant(b));

/** Whether cuilo ships the pack inside its binary. */
export function isBuiltIn(pack: Pack): boolean {
	return pack.source.startsWith("official:");
}

/** A pack's trust: built in for official packs; any other listed pack is signed until the directory reviews it. */
export function packTrust(pack: Pack): Trust {
	return isBuiltIn(pack) ? "builtIn" : "signed";
}

/**
 * The piece a pack leads with: its first piece that supports every stack the pack does and needs nothing beyond the
 * stack, such as a database library; else its first piece that supports every stack; else its first piece.
 */
export function leadPiece(pack: Pack): Piece | undefined {
	const stacks = packStacks(pack).length;
	const everyStack = pack.pieces.filter((piece) => pieceStacks(piece).length === stacks);
	return everyStack.find((piece) => piece.needs.length === 0) ?? everyStack[0] ?? pack.pieces[0];
}

/** The command that gets a pack into a project: adding its lead piece for a built-in pack, `cuilo pack add` otherwise. */
export function installCommand(pack: Pack): string {
	const piece = leadPiece(pack);
	return isBuiltIn(pack) && piece ? addCommand(pack.name, piece) : `cuilo pack add ${pack.source}`;
}

/** The files `cuilo add --dry-run` plans for a piece with one variant, as the generator records them. */
interface Example {
	variant: string;
	files: string[];
}

/**
 * The folder an example writes into, e.g. `src/ui/atoms/status-badge/`, with its variant's name: of `examples`, the
 * one whose variant comes first in priority.
 */
export function exampleOutput(examples: readonly Example[]): { folder: string; stack: string } | undefined {
	const [example] = examples
		.filter((candidate) => candidate.files.length > 0)
		.sort((a, b) => compareVariants(a.variant, b.variant));
	if (!example) {
		return;
	}
	const folders = example.files.map((file) => file.split("/").slice(0, -1));
	const [first = []] = folders;
	let depth = 0;
	while (depth < first.length && folders.every((parts) => parts[depth] === first[depth])) {
		depth += 1;
	}
	const common = first.slice(0, depth).join("/");
	return { folder: common ? `${common}/` : "./", stack: variantName(example.variant) };
}
