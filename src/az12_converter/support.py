"""Optional public support link; no payment processing runs inside the converter."""
from urllib.parse import urlsplit
import webbrowser
from tkinter import messagebox


# Public support page supplied by the project owner. Opens only on a user's click.
SUPPORT_URL = "https://paypal.me/12dazapp"
COFFEE_MESSAGE = "If you like the product, buy us a coffee."


def support_url():
    value = SUPPORT_URL.strip()
    try:
        parsed = urlsplit(value)
    except ValueError:
        return ''
    if any(char.isspace() for char in value):
        return ''
    if parsed.scheme != 'https' or not parsed.hostname or parsed.username or parsed.password:
        return ''
    return value


def open_support(parent=None):
    url = support_url()
    if not url:
        return False
    try:
        opened = webbrowser.open_new_tab(url)
    except (OSError, webbrowser.Error):
        opened = False
    if not opened:
        messagebox.showinfo('Support development', f'Open this public support page in your browser:\n\n{url}', parent=parent)
    return opened
