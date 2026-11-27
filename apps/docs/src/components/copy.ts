/** How long a copy button says it copied, in milliseconds. */
const COPIED_FOR_MS = 1500;

/**
 * Makes every `[data-copy]` button inside `root` copy its `data-copy` text. Its `[data-copy-label]` says `copied` for a
 * moment, and `status`, a live region, announces it.
 */
export function wireCopyButtons(root: HTMLElement, copied: string, status?: HTMLElement | null) {
	root.addEventListener("click", async (event) => {
		const button = event.target instanceof Element ? event.target.closest<HTMLElement>("[data-copy]") : null;
		const text = button?.dataset.copy;
		if (!(button && text)) {
			return;
		}
		event.preventDefault();
		await navigator.clipboard.writeText(text);
		const label = button.querySelector<HTMLElement>("[data-copy-label]");
		const idle = label?.textContent ?? "";
		if (label) {
			label.textContent = copied;
		}
		button.toggleAttribute("data-copied", true);
		if (status) {
			status.textContent = `${copied}: ${text}`;
		}
		setTimeout(() => {
			if (label) {
				label.textContent = idle;
			}
			button.removeAttribute("data-copied");
		}, COPIED_FOR_MS);
	});
}
