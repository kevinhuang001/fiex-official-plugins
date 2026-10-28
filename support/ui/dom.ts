import { t } from "./i18n";

/** Only authored UI copy is marked. Resource names and server data are untouched. */
export function localizeDocument() {
  for (const element of document.querySelectorAll<HTMLElement>("[data-i18n]")) {
    element.textContent = t(element.dataset.i18n!);
  }
  for (const attribute of ["placeholder", "title", "aria-label"]) {
    for (const element of document.querySelectorAll(
      `[data-i18n-${attribute}]`,
    )) {
      element.setAttribute(
        attribute,
        t(element.getAttribute(`data-i18n-${attribute}`)!),
      );
    }
  }
}
