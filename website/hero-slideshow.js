(() => {
  const slides = [...document.querySelectorAll('.hero-slide')];
  const button = document.querySelector('.hero-pause');
  const captions = document.querySelectorAll('[data-hero-caption]');
  if (slides.length < 2 || !button) return;

  const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
  let paused = motion.matches;
  let current = 0;
  let timer;

  function sync() {
    clearInterval(timer);
    button.textContent = paused ? 'Play slideshow' : 'Pause slideshow';
    if (paused || document.hidden) return;
    timer = setInterval(() => {
      const next = (current + 1) % slides.length;
      // Keep the current scene if the next image has not loaded successfully.
      if (!slides[next].complete || !slides[next].naturalWidth) return;
      slides[current].classList.remove('is-active');
      slides[current].setAttribute('aria-hidden', 'true');
      slides[next].classList.add('is-active');
      slides[next].removeAttribute('aria-hidden');
      const credit = slides[next].dataset.heroCredit ?? 'illustration';
      captions.forEach(caption => { caption.hidden = caption.dataset.heroCaption !== credit; });
      current = next;
    }, 5000);
  }

  button.hidden = false;
  button.addEventListener('click', () => {
    paused = !paused;
    sync();
  });
  motion.addEventListener('change', () => {
    paused = motion.matches;
    sync();
  });
  document.addEventListener('visibilitychange', sync);
  sync();
})();
