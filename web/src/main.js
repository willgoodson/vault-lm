import { createApp } from 'vue';
import '@fontsource-variable/inter';
import './style.css';
import App from './App.vue';
import { routeExternalLinks } from './native.js';
routeExternalLinks();
createApp(App).mount('#app');
