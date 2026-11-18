// The page the person is looking at: the active tab of the last focused
// normal browser window (not a popup or an extension window).
export async function activePage() {
  try {
    const win = await chrome.windows.getLastFocused({ windowTypes: ["normal"] });
    const [tab] = await chrome.tabs.query({ active: true, windowId: win.id });
    return [tab];
  } catch {
    return [];
  }
}
