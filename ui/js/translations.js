// Translation picker dialog: every translation in the library, grouped, with what
// it is and the credit its licence asks for.

import { h, icon, replace } from "./dom.js";

let dialog, body;
let onPick = () => {};
let picking = onPick;

export function initTranslations(pick) {
  onPick = pick;
  dialog = document.getElementById("translations");
  body = document.getElementById("translations-body");
  const close = document.getElementById("translations-close");
  close.append(icon("close"));
  close.addEventListener("click", () => dialog.close());
  dialog.addEventListener("click", (event) => {
    if (event.target === dialog) dialog.close();
  });
}

/** Show the picker with `current` (a translation id) marked. A pick goes to `pick`
 * when given (choosing a translation for something other than reading), else to the
 * reader. `title` replaces the heading for the while. */
export function openTranslations(bibles, current, { pick = null, title = "Translations" } = {}) {
  picking = pick ?? onPick;
  document.getElementById("translations-title").textContent = title;
  const filter = h("input", {
    type: "search",
    placeholder: "Find a translation",
    "aria-label": "Find a translation",
    autocomplete: "off",
    spellcheck: "false",
  });
  const lists = h("div");
  const draw = () => {
    const q = filter.value.trim().toLowerCase();
    const matches = (b) =>
      !q || b.abbr.toLowerCase().includes(q) || b.name.toLowerCase().includes(q) || b.year.toLowerCase().includes(q) || b.group.toLowerCase().includes(q);
    const groups = [];
    for (const b of bibles.filter(matches)) {
      let g = groups.find((x) => x.name === b.group);
      if (!g) groups.push((g = { name: b.group, items: [] }));
      g.items.push(b);
    }
    replace(
      lists,
      groups.length
        ? groups.map((g) => [h("h3", { class: "section-title" }, g.name), h("ul", { class: "translation-list" }, g.items.map((b) => item(b, current)))])
        : h("p", { class: "empty" }, "No translation matches."),
    );
  };
  filter.addEventListener("input", draw);
  filter.addEventListener("keydown", (event) => {
    if (event.key === "Enter") lists.querySelector(".translation-list button")?.click();
  });
  draw();
  replace(body, h("div", { class: "field" }, filter), lists);
  dialog.showModal();
  const active = body.querySelector('[aria-current="true"]');
  (active ?? filter).focus();
  active?.scrollIntoView({ block: "center" });
}

function item(b, current) {
  return h(
    "li",
    null,
    h(
      "button",
      {
        type: "button",
        class: "translation-item",
        "aria-current": b.id === current ? "true" : null,
        onclick: () => {
          dialog.close();
          picking(b.id);
        },
      },
      h("span", { class: "translation-abbr" }, b.abbr),
      h(
        "span",
        { class: "translation-text" },
        h("span", { class: "translation-name" }, b.name, h("span", { class: "translation-year" }, ` · ${b.year}`)),
        h("span", { class: "translation-about" }, b.about),
      ),
    ),
  );
}
