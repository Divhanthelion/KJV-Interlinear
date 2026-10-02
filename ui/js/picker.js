// Book and chapter picker dialog.

import { h, icon, replace } from "./dom.js";

let dialog, body, title, backButton;
let books = [];
let current = { book: "", chapter: 0 };
let onPick = () => {};
let stage = "books";

export function initPicker(allBooks, pick) {
  books = allBooks;
  onPick = pick;
  dialog = document.getElementById("picker");
  body = document.getElementById("picker-body");
  title = document.getElementById("picker-title");
  backButton = document.getElementById("picker-back");
  backButton.append(icon("arrowLeft"));
  const close = document.getElementById("picker-close");
  close.append(icon("close"));
  close.addEventListener("click", () => dialog.close());
  backButton.addEventListener("click", showBooks);
  // Click on the backdrop closes the dialog
  dialog.addEventListener("click", (event) => {
    if (event.target === dialog) dialog.close();
  });
  // Escape on the chapter grid goes back to the books instead of closing
  dialog.addEventListener("cancel", (event) => {
    if (stage === "chapters") {
      event.preventDefault();
      showBooks();
    }
  });
}

/** The books to offer (the current translation's). */
export function setPickerBooks(list) {
  books = list;
}

export function isPickerOpen() {
  return dialog?.open ?? false;
}

export function openPicker(book, chapter) {
  current = { book, chapter };
  showBooks();
  dialog.showModal();
  // Put focus on the current book so keyboard users start where they are
  const active = body.querySelector('[aria-current="true"]');
  (active ?? body.querySelector("input"))?.focus();
  active?.scrollIntoView({ block: "center" });
}

export function closePicker() {
  if (dialog.open) dialog.close();
}

function bookButton(book) {
  return h(
    "button",
    {
      type: "button",
      "aria-current": book.name === current.book ? "true" : null,
      onclick: () => (book.chapters === 1 ? choose(book.name, numbersOf(book)[0]) : showChapters(book)),
    },
    book.display,
  );
}

function showBooks() {
  stage = "books";
  title.textContent = "Books";
  backButton.hidden = true;
  const filter = h("input", {
    type: "search",
    placeholder: "Find a book",
    "aria-label": "Find a book",
    autocomplete: "off",
    spellcheck: "false",
  });
  const lists = h("div");
  const draw = () => {
    const q = filter.value.trim().toLowerCase().replace(/\s+/g, " ");
    const matches = (b) =>
      !q || b.display.toLowerCase().includes(q) || b.abbr.toLowerCase().startsWith(q) || b.name.toLowerCase().includes(q);
    const section = (label, testament) => {
      const list = books.filter((b) => b.testament === testament && matches(b));
      return list.length
        ? [h("h3", { class: "section-title" }, label), h("div", { class: "book-grid" }, list.map(bookButton))]
        : [];
    };
    const content = [...section("Old Testament", "old"), ...section("Apocrypha", "apocrypha"), ...section("New Testament", "new")];
    replace(lists, content.length ? content : h("p", { class: "empty" }, "No book matches."));
  };
  filter.addEventListener("input", draw);
  filter.addEventListener("keydown", (event) => {
    // Enter opens the only (or first) match
    if (event.key === "Enter") lists.querySelector(".book-grid button")?.click();
  });
  draw();
  replace(body, h("div", { class: "field" }, filter), lists);
}

function showChapters(book) {
  stage = "chapters";
  title.textContent = book.display;
  backButton.hidden = false;
  const buttons = [];
  for (const n of numbersOf(book)) {
    buttons.push(
      h(
        "button",
        {
          type: "button",
          "aria-current": book.name === current.book && n === current.chapter ? "true" : null,
          "aria-label": `${book.display} ${n}`,
          onclick: () => choose(book.name, n),
        },
        n,
      ),
    );
  }
  replace(body, h("div", { class: "chapter-grid" }, buttons));
  (body.querySelector('[aria-current="true"]') ?? body.querySelector("button"))?.focus();
}

/** A book's chapter numbers: 1..n unless the translation numbers them otherwise. */
function numbersOf(book) {
  return book.numbers ?? Array.from({ length: book.chapters }, (_, i) => i + 1);
}

function choose(book, chapter) {
  dialog.close();
  onPick(book, chapter);
}
