/** The example in an input's description, e.g. `blog-post` in "Singular name, e.g. `blog-post`." */
const EXAMPLE = /e\.g\. `([^`]+)`/;

/** The parts of a piece in `src/data/packs.json` that its command needs. */
interface PieceInputs {
	name: string;
	inputs: { name: string; description?: string | null }[];
}

/** `cuilo add <pack>/<piece>`, followed by the example its `name` input gives, when it has one. */
export const addCommand = (pack: string, piece: PieceInputs) => {
	const example = piece.inputs.find((input) => input.name === "name")?.description?.match(EXAMPLE)?.[1];
	return ["cuilo add", `${pack}/${piece.name}`, example].filter(Boolean).join(" ");
};
