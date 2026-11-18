// Asks once for the microphone on the extension's own page, which a side
// panel cannot do (spec FR-29; plan 1).

const state = document.getElementById("state");
document.getElementById("allow").addEventListener("click", async () => {
  try {
    const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    for (const track of stream.getTracks()) track.stop();
    state.textContent = "Done. Go back to the side panel and press the microphone.";
    setTimeout(() => window.close(), 1500);
  } catch (error) {
    state.textContent = `The microphone was not allowed: ${error.message}`;
  }
});
