// App-local light/dark theme source. Replaces the blog's theme store for the
// ported chart components: follows the OS preference and stamps `data-theme`
// on <html> so the CSS variables (styles.css) switch with it.

import { reactive } from 'vue';

export const theme = reactive({ name: 'light' as 'light' | 'dark' });

function apply(dark: boolean) {
  theme.name = dark ? 'dark' : 'light';
  document.documentElement.dataset.theme = theme.name;
}

export function initTheme() {
  const media = window.matchMedia('(prefers-color-scheme: dark)');
  apply(media.matches);
  media.addEventListener('change', (e) => apply(e.matches));
}
