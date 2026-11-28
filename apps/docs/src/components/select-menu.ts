/** How long typed letters keep adding up into one search, in milliseconds. */
const TYPEAHEAD_MS = 500;
/** Space between the button and its list, in pixels. */
const GAP_PX = 6;
/** The least space the list keeps from the window's edges, in pixels. */
const EDGE_PX = 8;
/** The smallest height the list shrinks to when neither side has room, in pixels. */
const MIN_HEIGHT_PX = 120;
/** How many options PageUp and PageDown move over. */
const PAGE = 10;
/** Accents, removed so typing `e` finds `Español`. */
const ACCENTS = /\p{Diacritic}/gu;
/** Keys that open a closed list. */
const OPENING_KEYS = new Set(["ArrowDown", "ArrowUp", "Enter", " ", "Home", "End"]);

/** `text` in lowercase without accents, for type-ahead matching. */
function folded(text: string): string {
	return text.toLowerCase().normalize("NFD").replace(ACCENTS, "");
}

/** Whether a key press types a character, without a modifier that makes it a shortcut. */
function typesCharacter(event: KeyboardEvent): boolean {
	return event.key.length === 1 && !(event.ctrlKey || event.metaKey || event.altKey);
}

/**
 * A select-only combobox, as in the WAI-ARIA Authoring Practices: a button that opens a listbox of options in a popover.
 * Its `value` attribute holds the chosen option's value; it fires `change` when the user picks another one. Options
 * that are links open their page instead.
 */
class SelectMenu extends HTMLElement {
	static observedAttributes = ["value"];

	#button: HTMLButtonElement | null = null;
	#list: HTMLElement | null = null;
	#active = -1;
	#typed = "";
	#typedAt = 0;
	readonly #reposition = () => requestAnimationFrame(() => this.#place());

	/** The chosen option's value. */
	get value(): string {
		return this.getAttribute("value") ?? "";
	}

	set value(value: string) {
		this.setAttribute("value", value);
	}

	connectedCallback() {
		this.#button = this.querySelector<HTMLButtonElement>("[data-select-button]");
		this.#list = this.querySelector<HTMLElement>("[role=listbox]");
		if (!(this.#button && this.#list)) {
			return;
		}
		this.#button.popoverTargetElement = this.#list;
		this.#button.addEventListener("keydown", (event) => this.#onKeyDown(event));
		this.#list.addEventListener("beforetoggle", (event) => this.#onBeforeToggle(event));
		this.#list.addEventListener("toggle", (event) => this.#onToggle(event));
		this.#list.addEventListener("mousedown", (event) => event.preventDefault());
		this.#list.addEventListener("click", (event) => this.#onClick(event));
		this.#list.addEventListener("pointermove", (event) => this.#onPointerMove(event));
		this.#render();
	}

	attributeChangedCallback() {
		this.#render();
	}

	/** Closes the list without choosing anything. */
	close() {
		if (this.#isOpen()) {
			this.#list?.hidePopover();
		}
	}

	#options(): HTMLElement[] {
		return [...(this.#list?.querySelectorAll<HTMLElement>("[role=option]") ?? [])];
	}

	#isOpen(): boolean {
		return this.#list?.matches(":popover-open") ?? false;
	}

	#chosenIndex(): number {
		const options = this.#options();
		const index = options.findIndex((option) => option.dataset.value === this.value);
		return Math.max(index, 0);
	}

	/** Marks the option matching `value` as selected and shows it in the button. */
	#render() {
		const options = this.#options();
		const chosen = options[this.#chosenIndex()];
		for (const option of options) {
			option.setAttribute("aria-selected", String(option === chosen));
		}
		const shown = this.querySelector<HTMLElement>("[data-select-value]");
		if (shown && chosen) {
			shown.textContent = chosen.dataset.short ?? chosen.querySelector("[data-option-label]")?.textContent ?? "";
		}
		for (const icon of this.querySelectorAll<HTMLElement>("[data-option-icon]")) {
			icon.hidden = icon.dataset.optionIcon !== chosen?.dataset.value;
		}
	}

	#open(index = this.#chosenIndex()) {
		if (!this.#isOpen()) {
			this.#list?.showPopover();
			this.#place();
		}
		this.#activate(index);
	}

	/** Highlights the option at `index`, clamped to the list, as the one Enter would choose. */
	#activate(index: number) {
		const options = this.#options();
		this.#active = Math.min(Math.max(index, 0), options.length - 1);
		for (const [position, option] of options.entries()) {
			option.toggleAttribute("data-active", position === this.#active);
		}
		const active = options[this.#active];
		if (active) {
			this.#button?.setAttribute("aria-activedescendant", active.id);
			active.scrollIntoView({ block: "nearest" });
		}
	}

	/** Chooses the option at `index` and closes the list; a link option opens its page instead. */
	#choose(index: number) {
		const option = this.#options()[index];
		this.close();
		if (!option) {
			return;
		}
		if (option instanceof HTMLAnchorElement) {
			globalThis.location.assign(option.href);
			return;
		}
		const value = option.dataset.value ?? "";
		if (value !== this.value) {
			this.value = value;
			this.dispatchEvent(new Event("change", { bubbles: true }));
		}
	}

	#onKeyDown(event: KeyboardEvent) {
		if (this.#isOpen()) {
			this.#onKeyWhileOpen(event);
		} else if (OPENING_KEYS.has(event.key)) {
			event.preventDefault();
			const edges = new Map([
				["Home", 0],
				["End", this.#options().length - 1],
			]);
			this.#open(edges.get(event.key));
		} else if (typesCharacter(event)) {
			this.#open();
			this.#typeahead(event.key);
		}
	}

	#onKeyWhileOpen(event: KeyboardEvent) {
		const last = this.#options().length - 1;
		const moves = new Map([
			["ArrowDown", this.#active + 1],
			["ArrowUp", this.#active - 1],
			["PageDown", this.#active + PAGE],
			["PageUp", this.#active - PAGE],
			["Home", 0],
			["End", last],
		]);
		const target = moves.get(event.key);
		if (target !== undefined) {
			event.preventDefault();
			this.#typed = "";
			this.#activate(target);
		} else if (event.key === " " && this.#isTyping()) {
			event.preventDefault();
			this.#typeahead(" ");
		} else if (event.key === "Enter" || event.key === " ") {
			event.preventDefault();
			this.#choose(this.#active);
		} else if (event.key === "Escape") {
			event.preventDefault();
			this.close();
		} else if (event.key === "Tab") {
			this.close();
		} else if (typesCharacter(event)) {
			this.#typeahead(event.key);
		}
	}

	#isTyping(): boolean {
		return this.#typed.length > 0 && Date.now() - this.#typedAt < TYPEAHEAD_MS;
	}

	/** Moves to the next option whose label starts with what was typed; repeating one letter cycles through matches. */
	#typeahead(character: string) {
		this.#typed = this.#isTyping() ? this.#typed + folded(character) : folded(character);
		this.#typedAt = Date.now();
		const repeated = [...this.#typed].every((letter) => letter === this.#typed[0]);
		const search = repeated ? this.#typed.slice(0, 1) : this.#typed;
		const options = this.#options();
		const start = repeated ? this.#active + 1 : this.#active;
		const ordered = [...options.slice(start), ...options.slice(0, start)];
		const match = ordered.find((option) =>
			folded(option.querySelector("[data-option-label]")?.textContent ?? "").startsWith(search)
		);
		if (match) {
			this.#activate(options.indexOf(match));
		}
	}

	#optionOf(event: Event): HTMLElement | undefined {
		return event.target instanceof Element
			? (event.target.closest<HTMLElement>("[role=option]") ?? undefined)
			: undefined;
	}

	#onClick(event: MouseEvent) {
		const option = this.#optionOf(event);
		if (!option) {
			return;
		}
		event.preventDefault();
		this.#choose(this.#options().indexOf(option));
	}

	#onPointerMove(event: PointerEvent) {
		const option = this.#optionOf(event);
		const index = option ? this.#options().indexOf(option) : -1;
		if (index !== -1 && index !== this.#active) {
			this.#activate(index);
		}
	}

	#onBeforeToggle(event: ToggleEvent) {
		const opening = event.newState === "open";
		this.#button?.setAttribute("aria-expanded", String(opening));
		if (opening) {
			this.#list?.removeAttribute("data-placed");
			addEventListener("scroll", this.#reposition, { capture: true, passive: true });
			addEventListener("resize", this.#reposition, { passive: true });
		} else {
			removeEventListener("scroll", this.#reposition, { capture: true });
			removeEventListener("resize", this.#reposition);
		}
	}

	#onToggle(event: ToggleEvent) {
		if (event.newState === "open") {
			this.#place();
			if (this.#active === -1) {
				this.#activate(this.#chosenIndex());
			}
			return;
		}
		this.#active = -1;
		this.#button?.removeAttribute("aria-activedescendant");
		for (const option of this.#options()) {
			option.removeAttribute("data-active");
		}
	}

	/** Puts the list under the button, or above it when only that side has room, inside the window's edges. */
	#place() {
		const button = this.#button;
		const list = this.#list;
		if (!(button && list && this.#isOpen())) {
			return;
		}
		const anchor = button.getBoundingClientRect();
		const width = document.documentElement.clientWidth;
		const height = document.documentElement.clientHeight;
		list.style.minWidth = `${Math.min(anchor.width, width - 2 * EDGE_PX)}px`;
		list.style.maxHeight = "";
		const below = height - anchor.bottom - GAP_PX - EDGE_PX;
		const above = anchor.top - GAP_PX - EDGE_PX;
		const top = list.offsetHeight > below && above > below;
		const room = top ? above : below;
		if (list.offsetHeight > room) {
			list.style.maxHeight = `${Math.max(room, MIN_HEIGHT_PX)}px`;
		}
		const start = this.dataset.align === "end" ? anchor.right - list.offsetWidth : anchor.left;
		const left = Math.min(Math.max(start, EDGE_PX), width - list.offsetWidth - EDGE_PX);
		list.style.left = `${Math.max(left, EDGE_PX)}px`;
		list.style.top = `${top ? anchor.top - GAP_PX - list.offsetHeight : anchor.bottom + GAP_PX}px`;
		list.dataset.side = top ? "top" : "bottom";
		list.toggleAttribute("data-placed", true);
	}
}

customElements.define("cu-select", SelectMenu);

addEventListener("pageshow", (event) => {
	if (event.persisted) {
		for (const select of document.querySelectorAll<SelectMenu>("cu-select")) {
			select.close();
		}
	}
});

export type { SelectMenu };
