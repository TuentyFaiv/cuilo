/** A theme choice: Auto follows the system's color scheme. */
type Theme = "auto" | "dark" | "light";

/** Key in `localStorage` to store the theme preference at, shared with Starlight's `ThemeProvider`. */
const storageKey = "starlight-theme";

/** A typesafe theme from any value; unknown values are coerced to `auto`. */
const parseTheme = (theme: unknown): Theme =>
	theme === "auto" || theme === "dark" || theme === "light" ? theme : "auto";

/** The user's stored preference. */
const loadTheme = (): Theme => parseTheme(typeof localStorage !== "undefined" && localStorage.getItem(storageKey));

/** Stores the user's preference; Auto is stored as an empty string, as Starlight does. */
function storeTheme(theme: Theme): void {
	if (typeof localStorage !== "undefined") {
		localStorage.setItem(storageKey, theme === "light" || theme === "dark" ? theme : "");
	}
}

/** The system's preferred color scheme. */
const preferredColorScheme = (): Theme => (matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark");

/** Shows `theme` in every picker, applies it to the document and stores it. */
function onThemeChange(theme: Theme): void {
	for (const picker of document.querySelectorAll<HTMLElement>("[data-theme-picker]")) {
		picker.dataset.choice = theme;
		picker.querySelector("cu-select")?.setAttribute("value", theme);
		for (const radio of picker.querySelectorAll<HTMLInputElement>("input")) {
			radio.checked = radio.value === theme;
		}
	}
	document.documentElement.dataset.theme = theme === "auto" ? preferredColorScheme() : theme;
	storeTheme(theme);
}

/** The theme a picker's `change` event chose: a radio's value or the select's. */
function chosenTheme(event: Event): Theme {
	const { target } = event;
	if (target instanceof HTMLInputElement) {
		return parseTheme(target.value);
	}
	return parseTheme(target instanceof HTMLElement ? target.getAttribute("value") : undefined);
}

matchMedia("(prefers-color-scheme: light)").addEventListener("change", () => {
	if (loadTheme() === "auto") {
		onThemeChange("auto");
	}
});

onThemeChange(loadTheme());
for (const picker of document.querySelectorAll<HTMLElement>("[data-theme-picker]")) {
	picker.addEventListener("change", (event) => onThemeChange(chosenTheme(event)));
}
