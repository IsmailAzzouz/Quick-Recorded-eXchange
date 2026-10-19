// Only use NoSleep.js's MIT-licensed embedded video assets, not its legacy
// page-reload workaround. Everything is bundled locally and works offline.
import sources from "nosleep.js/src/media.js";

export function createWakeVideo(page, interrupted) {
  let video;
  let wanted = false;
  let urls = [];
  return {
    async start() {
      wanted = true;
      if (!video) {
        video = page.createElement("video");
        video.className = "wake-video";
        video.setAttribute("aria-hidden", "true");
        video.setAttribute("playsinline", "");
        video.tabIndex = -1;
        video.disablePictureInPicture = true;
        video.loop = true;
        for (const type of ["webm", "mp4"]) {
          const data = atob(sources[type].split(",")[1]);
          const bytes = Uint8Array.from(data, (char) => char.charCodeAt(0));
          const url = URL.createObjectURL(
            new Blob([bytes], { type: `video/${type}` }),
          );
          urls.push(url);
          const source = page.createElement("source");
          source.src = url;
          source.type = `video/${type}`;
          video.append(source);
        }
        for (const event of ["pause", "ended", "error"]) {
          video.addEventListener(event, () => {
            if (wanted && (video.paused || video.ended || video.error))
              interrupted();
          });
        }
        page.body.append(video);
      }
      // Assets are silent. Leave the media element unmuted for older mobile
      // browsers whose no-sleep behavior depends on foreground video playback.
      if (video.paused) await video.play();
    },
    stop() {
      wanted = false;
      video?.pause();
    },
    dispose() {
      wanted = false;
      video?.pause();
      video?.remove();
      video = null;
      urls.forEach((url) => URL.revokeObjectURL(url));
      urls = [];
    },
  };
}
