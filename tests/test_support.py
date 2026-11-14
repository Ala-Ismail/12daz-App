import tkinter as tk
import unittest
from unittest.mock import Mock, patch

from az12_converter import support
from az12_converter.splash import StartupSplash, launch_gui


class SupportTests(unittest.TestCase):
    def test_public_support_url_only(self):
        for value in ['', 'file:///private/file', 'javascript:alert(1)', 'https://user:pass@example.com', 'https://[broken', 'https://example.com/has space']:
            with self.subTest(value=value), patch.object(support, 'SUPPORT_URL', value):
                self.assertEqual(support.support_url(), '')
        with patch.object(support, 'SUPPORT_URL', 'https://www.paypal.me/example'):
            self.assertEqual(support.support_url(), 'https://www.paypal.me/example')

    def test_link_opens_only_on_explicit_action(self):
        with patch.object(support, 'SUPPORT_URL', 'https://example.com/support'), patch.object(support.webbrowser, 'open_new_tab', return_value=True) as browser:
            support.support_url()
            browser.assert_not_called()
            self.assertTrue(support.open_support())
            browser.assert_called_once_with('https://example.com/support')

    def test_unconfigured_link_does_not_open_browser(self):
        with patch.object(support, 'SUPPORT_URL', ''), patch.object(support.webbrowser, 'open_new_tab') as browser:
            self.assertFalse(support.open_support())
            browser.assert_not_called()


class SplashTests(unittest.TestCase):
    def setUp(self):
        self.root = tk.Tk()
        self.root.withdraw()
        self.addCleanup(self.cleanup)

    def cleanup(self):
        try:
            for timer in self.root.tk.call('after', 'info'):
                self.root.after_cancel(timer)
            self.root.destroy()
        except tk.TclError:
            pass

    def test_splash_dismisses_without_support(self):
        callback = Mock()
        with patch.object(support, 'SUPPORT_URL', ''):
            splash = StartupSplash(self.root, callback, show=False)
        self.assertEqual(splash.support_button.cget('state'), 'disabled')
        splash.dismiss()
        splash.dismiss()
        callback.assert_called_once()

    def test_splash_auto_dismisses(self):
        callback = Mock()
        splash = StartupSplash(self.root, callback, duration_ms=0, show=False)
        self.root.update()
        self.assertTrue(splash.dismissed)
        callback.assert_called_once()

    def test_skip_before_initialization_reveals_when_ready(self):
        factory = Mock()
        with patch.object(self.root, 'deiconify') as reveal, patch.object(self.root, 'lift'):
            splash = launch_gui(self.root, factory, show_splash=False)
            splash.dismiss()
            reveal.assert_not_called()
            self.root.update()
            factory.assert_called_once_with(self.root)
            reveal.assert_called_once()

    def test_configured_support_button_is_enabled(self):
        with patch.object(support, 'SUPPORT_URL', 'https://example.com/support'), patch.object(support.webbrowser, 'open_new_tab', return_value=True) as browser:
            splash = StartupSplash(self.root, Mock(), show=False)
            self.assertEqual(splash.support_button.cget('state'), 'normal')
            browser.assert_not_called()
            splash.support_button.invoke()
            browser.assert_called_once_with('https://example.com/support')
            splash.dismiss()
