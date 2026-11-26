/**
 * Selects the install method last chosen on the home or install page before first paint, as Starlight's synced tabs do.
 * The hero inlines it right after the install card.
 */
(() => {
	const card = document.currentScript?.previousElementSibling;
	const saved = typeof localStorage === "undefined" ? null : localStorage.getItem("starlight-synced-tabs__install");
	const tabs = [...(card?.querySelectorAll("[data-synced]") ?? [])];
	const chosen = tabs.findIndex((tab) => tab.dataset.synced === saved);
	if (chosen < 1) {
		return;
	}
	for (const [index, tab] of tabs.entries()) {
		tab.setAttribute("aria-selected", String(index === chosen));
		tab.tabIndex = index === chosen ? 0 : -1;
		const panel = document.getElementById(tab.getAttribute("aria-controls") ?? "");
		if (panel) {
			panel.hidden = index !== chosen;
		}
	}
})();
