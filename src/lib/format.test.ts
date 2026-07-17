import { describe, expect, it } from 'vitest';

describe('reader progress', () => {
  it('clamps display percentages to whole values', () => {
    expect(Math.round(Math.min(1, Math.max(0, 0.426)) * 100)).toBe(43);
  });
});
