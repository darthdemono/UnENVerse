/**
 * Tools -> Import -> "From another password manager".
 *
 * R09 made these imports durable source migrations: a complete import writes
 * the vault, source identities and encrypted binary archives in one SQLCipher
 * transaction. The previous renderer-side quick importer could only replace the
 * entry array, so it is intentionally disabled rather than pretend that a
 * Bitwarden attachment, a 1Password document or a KDBX history was imported.
 */
import { inTauri } from './state';

export function initVendorImportPane(): void {
  const vendor = document.getElementById('vi-vendor') as HTMLSelectElement;
  const folders = document.getElementById('vi-folders') as HTMLInputElement;
  const fileBtn = document.getElementById('vi-file-btn') as HTMLButtonElement | null;
  const applyBtn = document.getElementById('vi-apply-btn') as HTMLButtonElement | null;
  const status = document.getElementById('vi-status');
  if (!fileBtn || !applyBtn || !status) return;
  vendor.disabled = true;
  folders.disabled = true;
  if (!inTauri) {
    status.textContent =
      'Full-fidelity source migration is CLI-only. In a terminal: unv import-vault.';
    fileBtn.disabled = true;
    return;
  }
  // The old Tauri bridge has no transaction that can couple persist() to a
  // migration manifest and vault_blobs. Disable it until that bridge exists.
  status.textContent =
    'Full-fidelity source migration is CLI-only. It is implemented from published formats and awaits sanitized real-export verification. In a terminal: unv import-vault.';
  fileBtn.disabled = true;
  applyBtn.disabled = true;
}
