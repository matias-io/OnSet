import {describe, expect, it} from 'vitest';
import {percent, DETAIL_LIMIT} from './AiAdviser';
import {cell} from './StructureReads';

describe('AI adviser', () => {
  it('shows agreement as a whole percentage', () => {
    expect(percent(2 / 3)).toBe('67%');
    expect(percent(null)).toBe('—');
  });
  it('limits details lookups to a manageable batch', () => {
    expect(DETAIL_LIMIT).toBe(25);
  });
});

describe('structure reads table', () => {
  it('shows a bar, none, or a question mark', () => {
    expect(cell({state: 'estimated', bar: 16})).toBe('bar 16');
    expect(cell({state: 'absent', bar: null})).toBe('none');
    expect(cell({state: 'unknown', bar: null})).toBe('?');
    expect(cell(undefined)).toBe('—');
  });
});
