// The Google Play edition is built with VITE_EDITION=play (see android-play.sh); the backend
// is built with the matching `play` Cargo feature and rejects links that need yt-dlp.
export const play: boolean = import.meta.env?.VITE_EDITION === "play";
