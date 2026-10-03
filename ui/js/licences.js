// The Licences page: everything the app carries and the terms it comes under, the same
// as NOTICE and THIRD-PARTY-SOFTWARE.md (both written by `kjv-import notices`).

import { openExternal } from "./backend.js";
import { APP, LICENCE_TEXT } from "./brand.js";
import { loadCatalogues } from "./context.js";
import { h, icon, plural, replace } from "./dom.js";

/** What each licence is called, where it is, and what it asks (as in NOTICE) */
const LICENCES = [
  ["pd", "Public domain", null, "Free of copyright: no conditions."],
  ["cc-by-4.0", "CC BY 4.0", "https://creativecommons.org/licenses/by/4.0/", "Free to share and adapt, with credit and a note of any changes."],
  ["cc-by-sa-4.0", "CC BY-SA 4.0", "https://creativecommons.org/licenses/by-sa/4.0/", "Free to share and adapt, with credit and a note of any changes; adaptations under the same licence."],
  ["cc-by-nd-4.0", "CC BY-ND 4.0", "https://creativecommons.org/licenses/by-nd/4.0/", "Free to share unaltered, with credit."],
  ["cc-by-nc-nd-4.0", "CC BY-NC-ND 4.0", "https://creativecommons.org/licenses/by-nc-nd/4.0/", "Free to share unaltered, with credit, and not for commercial use."],
];

let dialog = null;
let body = null;
let software = null; // Promise of { packages, texts }

function link(text, url) {
  return h("button", { type: "button", class: "text-link", onclick: () => openExternal(url) }, text);
}

function shell() {
  if (dialog) return dialog;
  const close = h("button", { class: "icon-btn", type: "button", "aria-label": "Close" }, icon("close"));
  body = h("div", { class: "picker-body licences-body" });
  dialog = h(
    "dialog",
    { class: "picker", id: "licences", "aria-labelledby": "licences-title" },
    h("div", { class: "picker-header" }, h("h2", { class: "picker-title", id: "licences-title" }, "Licences"), close),
    body,
  );
  close.addEventListener("click", () => dialog.close());
  dialog.addEventListener("click", (event) => {
    if (event.target === dialog) dialog.close();
  });
  document.getElementById("app").append(dialog);
  return dialog;
}

/** A text to show on request (a licence's full wording). */
function shown(label, text) {
  return h("details", { class: "licence-text" }, h("summary", {}, label), h("pre", { tabindex: "0" }, text));
}

/** Works grouped by licence: each with its name and the credit it asks for. */
function works(title, list) {
  const groups = LICENCES.map(([key, name, url, asks]) => {
    const mine = list.filter((w) => w.licence === key);
    if (!mine.length) return null;
    return h(
      "div",
      { class: "licence-group" },
      h("h4", { class: "licence-name" }, url ? link(name, url) : name, h("span", { class: "muted" }, ` · ${plural(mine.length, "work")}`)),
      h("p", { class: "licence-asks" }, asks),
      h(
        "ul",
        { class: "licence-works" },
        mine.map((w) => h("li", {}, h("span", { class: "licence-work" }, w.title), h("span", { class: "licence-credit" }, w.credit))),
      ),
    );
  });
  return h("section", { class: "licence-section" }, h("h3", { class: "section-title" }, title), groups);
}

async function softwareList(container) {
  replace(container, h("p", { class: "muted small" }, "Loading…"));
  try {
    software ??= fetch("software.json").then((r) => {
      if (!r.ok) throw new Error(`software.json: ${r.status}`);
      return r.json();
    });
    const s = await software;
    const filter = h("input", { type: "search", placeholder: "Find a package", "aria-label": "Find a package", autocomplete: "off", spellcheck: "false" });
    const list = h("ul", { class: "licence-packages" });
    const draw = () => {
      const q = filter.value.trim().toLowerCase();
      const matching = s.packages.filter((p) => !q || p.name.toLowerCase().includes(q) || p.license.toLowerCase().includes(q));
      replace(
        list,
        matching.map((p) => {
          const d = h("details", { class: "licence-package" }, h("summary", {}, h("span", { class: "licence-work" }, `${p.name} ${p.version}`), h("span", { class: "muted" }, ` · ${p.license}`)));
          // The texts are drawn when opened: there are hundreds
          d.addEventListener("toggle", () => {
            if (!d.open || d.dataset.drawn) return;
            d.dataset.drawn = "1";
            d.append(p.url ? h("p", { class: "small" }, link(p.url, p.url)) : null, ...p.texts.map((i) => h("pre", { tabindex: "0" }, s.texts[i])));
          });
          return h("li", {}, d);
        }),
      );
    };
    filter.addEventListener("input", draw);
    draw();
    replace(container, h("div", { class: "field" }, filter), list);
  } catch (error) {
    replace(container, h("p", { class: "chat-error" }, `Couldn’t load the list: ${error.message ?? error}`));
  }
}

/** Open the Licences page. */
export async function openLicences(ctx) {
  const d = shell();
  replace(body, h("p", { class: "muted" }, "Loading…"));
  if (!d.open) d.showModal();
  let known = { commentaries: [], crossrefs: [] };
  try {
    known = await loadCatalogues();
  } catch {
    // shown without them
  }
  const bibles = ctx.state.bibles.map((b) => ({ licence: b.licence, title: `${b.abbr} · ${b.name}, ${b.year}`, credit: b.credit }));
  const commentaries = known.commentaries.map((c) => ({ licence: c.licence, title: `${c.name} · ${c.author} (${c.year})`, credit: c.credit }));
  const crossrefs = known.crossrefs.map((c) => ({ licence: c.licence, title: c.name, credit: c.credit }));
  const softwareBox = h("div", { class: "licence-software" }, h("button", { type: "button", class: "button", onclick: () => softwareList(softwareBox) }, "Show the packages and their licences"));
  replace(
    body,
    h(
      "section",
      { class: "licence-section" },
      h("h3", { class: "section-title" }, APP.name),
      h("p", {}, `${APP.name} is free, and always will be: no ads, no accounts, nothing to buy. Its own code is free for anyone to use for anything, under the ${APP.licence} licence.`),
      shown("The licence", LICENCE_TEXT),
      h("p", { class: "small muted" }, "Everything below comes with it under its own terms, which that licence doesn’t change."),
    ),
    works("Bible translations", bibles),
    works("Commentaries", commentaries),
    works("Cross-references", crossrefs),
    h(
      "section",
      { class: "licence-section" },
      h("h3", { class: "section-title" }, "Hebrew, Aramaic, Greek, and the KJV"),
      h(
        "ul",
        { class: "licence-works" },
        h(
          "li",
          {},
          h("span", { class: "licence-work" }, "Hebrew, Aramaic, and Greek texts and lexicons (TAHOT, TAGNT, TBESH, TBESG)"),
          h("span", { class: "licence-credit" }, "STEP Bible (www.STEPBible.org). Data created initially by Tyndale House Cambridge, now curated by STEP Bible. Licensed under ", link("CC BY 4.0", "https://creativecommons.org/licenses/by/4.0/"), ". Read as the app needs: keyed to the KJV’s verses, the Greek following the Textus Receptus behind the KJV."),
        ),
        h(
          "li",
          {},
          h("span", { class: "licence-work" }, "The King James Version, 1769 text, and its words of Christ"),
          h("span", { class: "licence-credit" }, "Public domain outside the United Kingdom, where the King James Version is subject to Crown letters patent. Text: eBible.org / CrossWire Bible Society."),
        ),
      ),
    ),
    h(
      "section",
      { class: "licence-section" },
      h("h3", { class: "section-title" }, "Fonts"),
      h(
        "ul",
        { class: "licence-works" },
        h("li", {}, h("span", { class: "licence-work" }, "Noto Sans"), h("span", { class: "licence-credit" }, "Copyright 2015-2021 Google LLC. All Rights Reserved. SIL Open Font License 1.1.")),
        h("li", {}, h("span", { class: "licence-work" }, "Noto Sans Hebrew"), h("span", { class: "licence-credit" }, "Copyright 2019 Google Inc. All Rights Reserved. SIL Open Font License 1.1.")),
      ),
      fontLicence(),
    ),
    h(
      "section",
      { class: "licence-section" },
      h("h3", { class: "section-title" }, "Open-source software"),
      h("p", {}, `${APP.name} is built from open-source packages (Tauri and the Rust libraries it and the app use), each under its own licence: MIT, Apache 2.0, BSD, ISC, Zlib, Unicode, Boost, MPL 2.0, and others. Their licences ask that their notices come with the app; here they are.`),
      softwareBox,
    ),
  );
}

/** The Open Font License, read when opened */
function fontLicence() {
  const d = h("details", { class: "licence-text" }, h("summary", {}, "The SIL Open Font License"));
  d.addEventListener("toggle", async () => {
    if (!d.open || d.dataset.drawn) return;
    d.dataset.drawn = "1";
    try {
      const text = await fetch("fonts/OFL.txt").then((r) => r.text());
      d.append(h("pre", { tabindex: "0" }, text));
    } catch (error) {
      d.append(h("p", { class: "chat-error" }, String(error.message ?? error)));
    }
  });
  return d;
}
