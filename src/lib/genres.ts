import type { Book } from '../types';

export interface GenreDefinition {
  slug: string;
  name: string;
  keywords: string[];
}

export const HOME_GENRES: GenreDefinition[] = [
  {
    slug: 'science-fiction',
    name: 'Science Fiction',
    keywords: ['science fiction', 'dystopian', 'space opera', 'cyberpunk', 'alien', 'foundation', 'dune']
  },
  {
    slug: 'fantasy',
    name: 'Fantasy',
    keywords: ['fantasy', 'epic fantasy', 'middle-earth', 'harry potter', 'the chronicles of narnia']
  },
  {
    slug: 'action-adventure',
    name: 'Action & Adventure',
    keywords: ['action', 'adventure', 'thriller', 'spy fiction', 'espionage', 'superhero fiction', 'crime fiction', 'james bond']
  },
  {
    slug: 'drama',
    name: 'Drama',
    keywords: ['drama', 'plays', 'tragedy', 'theatre', 'shakespeare']
  },
  {
    slug: 'history',
    name: 'History',
    keywords: ['history', 'historical', 'mughal empire', 'military history', 'sports history']
  }
];

export function genreBySlug(slug: string | null) {
  return HOME_GENRES.find((genre) => genre.slug === slug);
}

export function bookMatchesGenre(book: Book, genre: GenreDefinition) {
  const tags = book.tags.map((tag) => tag.toLocaleLowerCase());
  return genre.keywords.some((keyword) => tags.some((tag) => tag.includes(keyword)));
}
