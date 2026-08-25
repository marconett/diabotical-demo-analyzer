import { createApp } from 'vue';
import App from './App.vue';
import './styles.css';

// Native apps don't expose a browser context menu; keep it in dev for the
// inspector.
if (!import.meta.env.DEV) {
  window.addEventListener('contextmenu', (e) => e.preventDefault());
}

createApp(App).mount('#app');
