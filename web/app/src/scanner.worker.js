import jsQR from "jsqr";

self.onmessage = ({ data: { id, pixels, width, height } }) => {
  try {
    const result = jsQR(new Uint8ClampedArray(pixels), width, height, {
      inversionAttempts: "dontInvert",
    });
    self.postMessage({ id, text: result?.data || "" });
  } catch (error) {
    self.postMessage({ id, error: String(error) });
  }
};
