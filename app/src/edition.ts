// The Google Play edition is built with VITE_EDITION=play (see android-play.sh); the backend
// is built with the matching `play` Cargo feature and rejects links that need yt-dlp.
export const play: boolean = import.meta.env?.VITE_EDITION === "play";

/**
 * Colors of the header logo in the Play edition, by the `data-logo` role of each SVG part. They are
 * set as attributes because the WebView paints gradient stops given through CSS variables black.
 */
export const PLAY_LOGO_COLORS: Readonly<Record<string, string>> = {
  start: "#7c3aed",
  end: "#5b21b6",
  fold: "#ede9fe",
  shadow: "#2e1065",
};
