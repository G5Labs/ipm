/**
 * Writes text to the clipboard. Resolves false, never rejects, when the
 * Clipboard API is missing (an insecure origin) or the browser refuses the
 * write (an unfocused page, a denied permission), so callers can say so.
 */
export async function writeClipboard(text: string): Promise<boolean> {
  try {
    if (!navigator.clipboard?.writeText) return false;
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}
