/** `play` has neither extensions nor link downloads; any other value is the `github` edition. */
export function isGithubEdition(edition: string | undefined): boolean {
  return edition !== "play";
}

export const githubEdition = isGithubEdition(import.meta.env?.VITE_EDITION);

/** Visible label that tells the github edition apart from the Play Store one. */
export function editionBadge(edition: string | undefined): string | null {
  return isGithubEdition(edition) ? "GitHub" : null;
}
