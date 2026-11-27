// Every web address opens in the system browser: the webview never navigates away from the app.

/** The address of a link that leaves the app, or `null` for anything the webview handles itself. */
export function externalAddress(href: string | null): string | null {
  return href !== null && /^https?:\/\//i.test(href) ? href : null;
}

/** Takes over the clicks on `http(s)` links, including those that open a new window. */
export function keepLinksOutside(open: (url: string) => Promise<void>, fail: (error: unknown) => void): void {
  document.addEventListener("click", (event) => {
    const link = event.target instanceof Element ? event.target.closest("a[href]") : null;
    const url = externalAddress(link?.getAttribute("href") ?? null);
    if (url === null) return;
    event.preventDefault();
    open(url).catch(fail);
  });
}
