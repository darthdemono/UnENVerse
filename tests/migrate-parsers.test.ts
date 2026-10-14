import { describe, expect, it } from 'vitest';
import fixture from './fixtures/parity/config-readers.json';
import { parseDockerCompose, parseNginxConf, parseWgConf } from '../src/ts/chunks/parsers';

describe('R08 parser parity fixture', () => {
  it('keeps the TypeScript fallback pinned while desktop migration preview uses Rust', () => {
    const readers = {
      wireguard: parseWgConf,
      compose: parseDockerCompose,
      nginx: parseNginxConf,
    } as const;
    for (const [format, expected] of Object.entries(fixture)) {
      const chunks = readers[format as keyof typeof readers](expected.text);
      expect(chunks.map((chunk) => chunk.chunk_type)).toEqual(expected.ts_chunks);
    }
  });
});
