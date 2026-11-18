"""Brief optional startup branding. It never appears in worker or smoke-test paths."""
import tkinter as tk

from . import COPYRIGHT, __version__
from .icons import make_icon
from . import support
from .ui_layout import BG, PANEL, BORDER, INK, MUTED, BLUE, PRIMARY


class StartupSplash:
    def __init__(self, root, on_dismiss, duration_ms=1800, *, show=True):
        self.root = root
        self.on_dismiss = on_dismiss
        self.dismissed = False
        self.startup_error = None
        self.window = tk.Toplevel(root, background=BG)
        if not show:
            self.window.withdraw()
        self.window.title('12dAZ Converter')
        self.window.overrideredirect(True)
        self.window.resizable(False, False)
        # Keep the splash separate from the hidden main window's geometry.
        shell = tk.Frame(self.window, background=BG, highlightthickness=1, highlightbackground=BORDER)
        shell.pack(fill='both', expand=True)
        inner = tk.Frame(shell, background=BG, padx=28, pady=24)
        inner.pack(fill='both', expand=True)
        self.logo = make_icon(self.window, 'logo', BLUE, 50)
        tk.Label(inner, image=self.logo, background=BG).pack(anchor='w', pady=(0, 12))
        tk.Label(inner, text='12dAZ Converter', background=BG, foreground=INK,
                 font=('Segoe UI', 22, 'bold')).pack(anchor='w')
        divider = tk.Frame(inner, background=BORDER, height=1)
        divider.pack(fill='x', pady=(18, 16))
        tk.Label(inner, text=support.COFFEE_MESSAGE, background=BG, foreground=INK,
                 font=('Segoe UI', 11), wraplength=430, justify='left').pack(anchor='w')
        tk.Label(inner, text='Free and open source. Support is optional.', background=BG, foreground=MUTED,
                 font=('Segoe UI', 9)).pack(anchor='w', pady=(5, 14))
        buttons = tk.Frame(inner, background=BG)
        buttons.pack(fill='x')
        self.support_button = tk.Button(buttons, text='Buy us a coffee (PayPal)', command=lambda: support.open_support(self.window),
            background=PRIMARY, foreground='white', activebackground='#2c7de7', activeforeground='white',
            relief='flat', borderwidth=0, font=('Segoe UI', 10, 'bold'), padx=15, pady=8,
            state='normal' if support.support_url() else 'disabled', disabledforeground='#9ba9bd',
            cursor='hand2' if support.support_url() else 'arrow', takefocus=True)
        self.support_button.pack(side='left')
        self.continue_button = tk.Button(buttons, text='Continue', command=self.dismiss,
            background=PANEL, foreground=INK, activebackground='#414c5c', activeforeground=INK,
            relief='flat', borderwidth=0, font=('Segoe UI', 10), padx=15, pady=8, takefocus=True)
        self.continue_button.pack(side='right')
        tk.Label(inner, text=f'v{__version__}  ·  Opening converter…', background=BG, foreground=MUTED,
                 font=('Segoe UI', 9)).pack(anchor='w', pady=(18, 8))
        tk.Label(inner, text=COPYRIGHT, background=BG, foreground=MUTED,
                 font=('Segoe UI', 8), wraplength=430, justify='left').pack(anchor='w')
        self.window.update_idletasks()
        width, height = self.window.winfo_reqwidth(), self.window.winfo_reqheight()
        x = max(0, (root.winfo_screenwidth() - width) // 2)
        y = max(0, (root.winfo_screenheight() - height) // 2)
        self.window.geometry(f'{width}x{height}+{x}+{y}')
        self.window.bind('<Escape>', lambda e: self.dismiss())
        self.window.bind('<Return>', lambda e: self.dismiss())
        self.window.protocol('WM_DELETE_WINDOW', self.dismiss)
        self.timer = root.after(duration_ms, self.dismiss)
        if show:
            self.window.after_idle(self.continue_button.focus_set)

    def dismiss(self):
        if self.dismissed:
            return
        self.dismissed = True
        self.root.after_cancel(self.timer)
        self.window.destroy()
        self.on_dismiss()


def launch_gui(root, app_factory, *, show_splash=True):
    """Show branding while the main UI is constructed, with an immediate skip."""
    root.withdraw()
    ready = [False]
    requested = [False]

    def reveal():
        requested[0] = True
        if ready[0]:
            root.deiconify()
            root.lift()

    splash = StartupSplash(root, reveal, show=show_splash)

    def initialize():
        try:
            app_factory(root)
            ready[0] = True
            if requested[0]:
                reveal()
        except Exception as exc:
            splash.startup_error = exc
            splash.dismiss()
            root.destroy()

    root.after(0, initialize)
    return splash
