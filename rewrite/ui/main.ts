import { mount } from 'svelte';
import App from './App.svelte';
import './style.css';
if (new URLSearchParams(location.search).get('mini') === '1') {
  void import('./Mini.svelte').then(({ default: Mini }) =>
    mount(Mini, { target: document.getElementById('app')! }),
  );
} else {
  mount(App, { target: document.getElementById('app')! });
}
