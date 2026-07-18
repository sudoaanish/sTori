import { describe, expect, it } from 'vitest';
import { uniqueBooks } from './DownloadsPage';

const book = (id: string) => ({ provider: 'Project Gutenberg', provider_id: id, title: id, authors: [], subjects: [], download_url: 'https://www.gutenberg.org/ebooks/1.epub', license_label: 'Public domain' });

describe('Gutenberg result merging', () => {
  it('suppresses duplicate editions while retaining earlier page results', () => {
    expect(uniqueBooks([book('1'), book('2'), book('1')]).map((item) => item.provider_id)).toEqual(['1', '2']);
  });
});
