const picker = document.getElementById("disk-image");
const expected = document.getElementById("expected-digest");
const result = document.getElementById("check-result");

if (globalThis.crypto?.subtle && typeof globalThis.File?.prototype.arrayBuffer === "function") {
  document.getElementById("file-check").hidden = false;
  document.querySelector(".manual-check").open = false;
  picker.addEventListener("change", checkDownload);
}

function showResult(message, state) {
  result.textContent = message;
  result.dataset.state = state;
}

async function checkDownload() {
  const file = picker.files[0];
  document.getElementById("chosen-name").textContent = file?.name ?? "";
  if (!file) {
    showResult("", "");
    return;
  }

  if (file.size !== Number(expected.dataset.bytes)) {
    showResult("Different file size. Download the v0.1.0 DMG again before opening it.", "mismatch");
    return;
  }

  showResult("Checking the file…", "checking");
  try {
    const bytes = await file.arrayBuffer();
    const hash = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
    if (picker.files[0] !== file) return;

    const hex = Array.from(hash, byte => byte.toString(16).padStart(2, "0")).join("");
    if (hex === expected.textContent.trim()) {
      showResult("Checksum matches the published BridgeVM v0.1.0 DMG.", "match");
    } else {
      showResult("Checksum mismatch. Download the v0.1.0 DMG again before opening it.", "mismatch");
    }
  } catch {
    if (picker.files[0] === file) {
      showResult("Your browser couldn’t read this file. Select it again or verify it in Terminal.", "error");
    }
  }
}
