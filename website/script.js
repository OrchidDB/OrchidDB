// The navigation remains visible when JavaScript is disabled.
const menu = document.querySelector('.menu-toggle');
const navigation = document.querySelector('#site-navigation');
if (menu && navigation) {
  document.documentElement.classList.add('js');
  menu.hidden = false;
  const closeMenu = () => {
    menu.setAttribute('aria-expanded', 'false');
    navigation.classList.remove('is-open');
  };
  menu.addEventListener('click', () => {
    const open = menu.getAttribute('aria-expanded') !== 'true';
    menu.setAttribute('aria-expanded', String(open));
    navigation.classList.toggle('is-open', open);
  });
  document.addEventListener('keydown', event => {
    if (event.key === 'Escape' && menu.getAttribute('aria-expanded') === 'true') {
      closeMenu();
      menu.focus();
    }
  });
  navigation.addEventListener('click', event => {
    if (event.target.closest('a')) closeMenu();
  });
}

const installOptions = document.querySelector('.install-options');
if (installOptions) {
  const buttons = [...installOptions.querySelectorAll('[data-install]')];
  const choose = name => {
    buttons.forEach(button => {
      const selected = button.dataset.install === name;
      button.setAttribute('aria-pressed', String(selected));
      document.getElementById('install-' + button.dataset.install).hidden = !selected;
    });
    document.querySelector('.copy-status').textContent = '';
  };
  installOptions.hidden = false;
  buttons.forEach(button => button.addEventListener('click', () => choose(button.dataset.install)));
  choose('cli');
  document.querySelectorAll('.copy-install').forEach(button => {
    button.hidden = false;
    button.addEventListener('click', async () => {
      const code = document.getElementById(button.dataset.copy).querySelector('pre code').textContent;
      try {
        await navigator.clipboard.writeText(code);
        document.querySelector('.copy-status').textContent = 'Command copied.';
      } catch {
        document.querySelector('.copy-status').textContent = 'Select the command above to copy it.';
      }
    });
  });
}
