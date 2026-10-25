// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * The settings window's tabs and the button that runs the welcome wizard again.
 */
import { describe, it, expect, beforeEach, afterAll } from 'vitest';
import { bootApp, entry, teardownApp } from './app-boot';

afterAll(() => teardownApp());

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const flush = (ms = 30) => new Promise((r) => setTimeout(r, ms));
type Boot = Awaited<ReturnType<typeof bootApp>>;
let Settings: Boot['Settings'];

beforeEach(async () => {
  ({ Settings } = await bootApp([entry('a', 'Alpha')]));
  $('settings-btn').click();
}, 30000);

describe('settings tabs', () => {
  it('opening a tab shows only its pane and marks only that tab', () => {
    const tabs = [...document.querySelectorAll<HTMLButtonElement>('.settings-tab')];
    expect(tabs.length).toBeGreaterThan(2);
    for (const tab of tabs) {
      tab.click();
      expect(tabs.filter((t) => t.classList.contains('active'))).toEqual([tab]);
      const active = [...document.querySelectorAll<HTMLElement>('.settings-tab-pane.active')];
      expect(active.map((p) => p.dataset.spane)).toEqual([tab.dataset.stab]);
    }
  });
});

describe('running the welcome wizard again', () => {
  const toLastStep = async () => {
    for (let n = 0; n < 12 && !document.getElementById('onboard-add'); n++) {
      $('onboard-next').click();
      await flush(10);
    }
  };

  it('closes settings first, then shows the wizard whose last step can add a secret', async () => {
    $('s-run-onboarding').click();
    await flush(80);
    expect($('settings-overlay').classList.contains('open')).toBe(false);
    expect(document.getElementById('onboard-title')).not.toBeNull();
    await toLastStep();
    $('onboard-add').click();
    await flush();
    expect($('modal-overlay').classList.contains('open')).toBe(true);
    expect(Settings.get('onboardingCompleted')).toBe(true);
  });

  it('its import button points at the load banner’s file picker', async () => {
    $('s-run-onboarding').click();
    await flush(80);
    await toLastStep();
    $('onboard-import').click();
    await flush();
    expect(document.querySelector('input[type=file]')).not.toBeNull();
  });
});
