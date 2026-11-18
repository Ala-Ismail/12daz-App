"""Desktop theme and three-panel layout."""
import math
import tkinter as tk
from tkinter import ttk
import sys

from . import __version__
from .icons import make_icon

BG = '#22252b'
PANEL = '#2b3038'
FIELD = '#232830'
BORDER = '#454e5b'
INK = '#e9edf4'
MUTED = '#a5afbe'
BLUE = '#4a97ff'
PRIMARY = '#2477dc'
PREVIEW = '#171c22'


def dark_titlebar(window):
    """Use dark non-client chrome where Windows supports per-window DWM colours."""
    if sys.platform != 'win32':
        return
    import ctypes
    from ctypes import wintypes
    try:
        user = ctypes.WinDLL('user32', use_last_error=True)
        user.GetParent.argtypes = [wintypes.HWND]
        user.GetParent.restype = wintypes.HWND
        hwnd = user.GetParent(window.winfo_id())
        dwm = ctypes.WinDLL('dwmapi', use_last_error=True)
        dwm.DwmSetWindowAttribute.argtypes = [wintypes.HWND, wintypes.DWORD, ctypes.c_void_p, wintypes.DWORD]
        enabled = ctypes.c_int(1)
        dwm.DwmSetWindowAttribute(hwnd, 20, ctypes.byref(enabled), ctypes.sizeof(enabled))
        for attribute, colour in [(35, BG), (36, INK)]:
            red, green, blue = bytes.fromhex(colour[1:])
            value = wintypes.DWORD(red | green << 8 | blue << 16)
            dwm.DwmSetWindowAttribute(hwnd, attribute, ctypes.byref(value), ctypes.sizeof(value))
    except (OSError, tk.TclError):
        pass  # Older Windows versions retain their native title bar.


def rounded_image(master, radius, fill, edge, outside):
    """Draw a rounded surface with a solid center that ttk can tile."""
    colours = [bytes.fromhex(colour[1:]) for colour in (outside, edge, fill)]
    # Tk tiles nine-slice images. A larger center avoids per-pixel drawing
    # calls when a surface spans a panel or the full width of the window.
    size = 2 * radius + 24
    image = tk.PhotoImage(master=master, width=size, height=size)
    distances = [max(0, radius - offset - .5, offset - (size - radius - .5))
                 for offset in range(size)]
    pixels = {}
    rows = []
    for dy in distances:
        row = []
        for dx in distances:
            if (dx, dy) not in pixels:
                distance = math.hypot(dx, dy)
                coverage = min(1, max(0, radius + .5 - distance))
                interior = min(1, max(0, radius - .5 - distance))
                rgb = [round(a * (1 - coverage) + b * (coverage - interior) + c * interior)
                       for a, b, c in zip(*colours)]
                pixels[dx, dy] = '#' + bytes(rgb).hex()
            row.append(pixels[dx, dy])
        rows.append('{' + ' '.join(row) + '}')
    image.put(' '.join(rows))
    return image


def configure_rounded_styles(app, style, scale):
    # A ttk image element keeps native bindings, state, and keyboard operation.
    images = app.surface_images = []

    def surface(name, radius, fill, edge, outside, states=()):
        radius = round(radius * scale)

        def picture(fill, edge):
            image = rounded_image(app.root, radius, fill, edge, outside)
            images.append(image)
            return image

        style.element_create(name, 'image', picture(fill, edge),
                             *((state, picture(fill, edge)) for state, fill, edge in states),
                             border=radius, padding=round(2 * scale), sticky='nswe',
                             width=2 * radius + 1, height=2 * radius + 1)

    for name, fill, edge, outside, radius in [
        ('Panel', PANEL, BORDER, BG, 9),
        ('File', FIELD, BORDER, BG, 7),
        ('Search', FIELD, BORDER, PANEL, 7),
        ('Preview', PREVIEW, PREVIEW, PANEL, 7),
    ]:
        states = [('alternate', FIELD, BLUE)] if name == 'Search' else ()
        surface(name + '.surface', radius, fill, edge, outside, states)
        style.layout(name + '.TFrame', [(name + '.surface', {'sticky': 'nswe'})])
        style.configure(name + '.TFrame', background=fill)

    button_states = [('disabled', '#2e343e', '#3b434f'), ('pressed', '#304b6f', BLUE),
                     ('focus', '#38414e', BLUE), ('active', '#404b5b', '#63788f')]
    for name, fill, edge, outside, states in [
        ('TButton', '#353d49', BORDER, BG, button_states),
        ('Open.TButton', '#353d49', BORDER, BG, button_states),
        ('Small.TButton', '#353d49', BORDER, PANEL, button_states),
        ('Primary.TButton', PRIMARY, PRIMARY, PANEL,
         [('disabled', '#3b4e68', '#3b4e68'), ('pressed', '#1769ca', BLUE),
          ('focus', PRIMARY, '#a0ccff'), ('active', '#2c7de7', '#2c7de7')]),
        ('Link.TButton', PANEL, PANEL, PANEL,
         [('focus', PANEL, BLUE), ('active', '#374355', '#374355')]),
        ('Warning.TButton', PANEL, PANEL, PANEL,
         [('focus', PANEL, BLUE), ('active', '#514330', '#514330')]),
        ('Menu.TButton', BG, BG, BG,
         [('pressed', '#394555', '#51627a'), ('focus', BG, BLUE), ('active', '#323b48', '#323b48')]),
        ('MenuItem.TButton', PANEL, PANEL, PANEL,
         [('pressed', '#345179', BLUE), ('active', '#354760', '#354760'),
          ('focus', '#354760', '#5d8cca')]),
    ]:
        element = name + '.surface'
        surface(element, 6, fill, edge, outside, states)
        style.layout(name, [(element, {'sticky': 'nswe', 'children': [
            ('Button.padding', {'sticky': 'nswe', 'children': [
                ('Button.label', {'sticky': 'nswe'})]})]})])

    surface('Input.surface', 6, FIELD, BORDER, PANEL,
            [('disabled', '#292e36', '#3c4450'), ('focus', FIELD, BLUE)])
    for name in ('Entry', 'Combobox'):
        children = [(name + '.textarea', {'sticky': 'nswe'})]
        if name == 'Combobox':
            children.insert(0, ('Combobox.downarrow', {'side': 'right', 'sticky': 'ns'}))
        style.layout('T' + name, [('Input.surface', {'sticky': 'nswe', 'children': [
            (name + '.padding', {'sticky': 'nswe', 'children': children})]})])


def configure_styles(app):
    root = app.root
    style = ttk.Style(root)
    style.theme_use('clam')
    style.configure('.', font=('Segoe UI', 10), background=BG, foreground=INK)
    style.configure('TFrame', background=BG)
    style.configure('Card.TFrame', background=PANEL)
    style.configure('Table.TFrame', background=FIELD)
    style.configure('TLabel', background=BG, foreground=INK)
    style.configure('Card.TLabel', background=PANEL)
    style.configure('Muted.TLabel', background=PANEL, foreground=MUTED, font=('Segoe UI', 9))
    style.configure('Title.TLabel', font=('Segoe UI', 18, 'bold'))
    style.configure('Heading.TLabel', background=PANEL, font=('Segoe UI', 11, 'bold'))
    style.configure('Status.TLabel', background=PANEL, font=('Segoe UI', 10, 'bold'))
    style.configure('TButton', padding=(9, 4))
    style.map('TButton', foreground=[('disabled', '#8190a3')])
    style.configure('Open.TButton', font=('Segoe UI', 10, 'bold'), padding=(10, 5))
    style.configure('Small.TButton', padding=(6, 3), font=('Segoe UI', 9), background=PANEL)
    style.configure('Link.TButton', padding=(4, 2), background=PANEL,
                    foreground=BLUE, font=('Segoe UI', 10, 'underline'))
    style.configure('Warning.TButton', padding=(6, 2), background=PANEL,
                    foreground='#f3bb60', font=('Segoe UI', 10, 'bold'))
    style.configure('Primary.TButton', foreground='white', background=PANEL,
                    font=('Segoe UI', 12, 'bold'), padding=(12, 8))
    style.map('Primary.TButton', foreground=[('disabled', '#b9c9e0')])
    style.configure('TEntry', padding=(6, 3), background=PANEL, insertcolor=INK)
    style.configure('TCombobox', padding=(6, 3), fieldbackground=FIELD, background=FIELD,
                    arrowsize=11, bordercolor=FIELD, lightcolor=FIELD, darkcolor=FIELD, arrowcolor='#bec9d9')
    style.map('TCombobox', background=[('disabled', '#292e36')],
              foreground=[('readonly', INK), ('disabled', '#93a0b3')], selectbackground=[('readonly', FIELD)],
              selectforeground=[('readonly', INK)])
    style.configure('TCheckbutton', background=PANEL, indicatorbackground=FIELD, indicatorforeground=BLUE)
    style.map('TCheckbutton', background=[('disabled', PANEL), ('active', PANEL)],
              foreground=[('disabled', '#9aa6b7')],
              indicatorbackground=[('disabled', '#343b46'), ('pressed', '#394b64')])
    ratio = max(1, root.winfo_fpixels('1i') / 96)
    row_height = round(27 * ratio)
    style.configure('Treeview', background=FIELD, fieldbackground=FIELD, borderwidth=0, rowheight=row_height,
                    foreground=INK, font=('Segoe UI', 10), bordercolor=BORDER, lightcolor=BORDER, darkcolor=BORDER)
    style.map('Treeview', background=[('selected', '#294f7c')], foreground=[('selected', '#f0f5fc')])
    style.configure('Treeview.Heading', background='#353c47', foreground='#dce5f2', padding=(6, 5),
                    font=('Segoe UI', 10, 'bold'), relief='flat', borderwidth=0)
    style.map('Treeview.Heading', background=[('active', '#444e5e')])
    style.configure('TPanedwindow', background=BG)
    style.configure('Horizontal.TProgressbar', troughcolor=BG, background=BLUE, borderwidth=0, thickness=3)
    style.configure('TScrollbar', background='#596371', troughcolor=PANEL, bordercolor=BORDER,
                    lightcolor=PANEL, darkcolor=PANEL, arrowcolor=INK, borderwidth=0, gripcount=0)
    style.configure('Vertical.TScrollbar', background='#596371', troughcolor=PANEL, bordercolor=BORDER,
                    lightcolor=PANEL, darkcolor=PANEL, arrowcolor=INK, borderwidth=0, arrowsize=12)
    style.map('TScrollbar', background=[('disabled', PANEL), ('pressed', '#74869b'), ('active', '#657489')],
              arrowcolor=[('disabled', '#637183')])
    root.configure(background=BG)
    style.configure('TSeparator', background=BORDER, bordercolor=BORDER, lightcolor=BORDER, darkcolor=BORDER)
    style.configure('Menu.TButton', foreground=MUTED, padding=(7, 1), font=('Segoe UI', 9))
    style.map('Menu.TButton', foreground=[('active', INK), ('focus', INK)])
    style.configure('MenuItem.TButton', background=PANEL, anchor='w', padding=(7, 4))
    configure_rounded_styles(app, style, ratio)
    root.option_add('*TCombobox*Listbox.background', FIELD)
    root.option_add('*TCombobox*Listbox.foreground', INK)
    root.option_add('*TCombobox*Listbox.selectBackground', '#294f7c')
    root.option_add('*TCombobox*Listbox.selectForeground', INK)
    app.icons = {}
    for name, colour in [('folder', BLUE), ('file', '#b4c1d3'), ('search', '#a3b2c7'), ('fit', '#cad4e3'),
                         ('zoom_in', '#cad4e3'), ('zoom_out', '#cad4e3'), ('export', '#ffffff'),
                         ('checked', BLUE), ('empty', '#929eae'), ('warning', '#e5ab43'),
                         ('success', '#40af71'), ('info', '#a2afc2')]:
        app.icons[name] = make_icon(root, name, colour, round(17 * ratio))
    app.icons['logo'] = make_icon(root, 'logo', BLUE, round(30 * ratio))


def build_menu_bar(app, px):
    root = app.root
    bar = ttk.Frame(root, padding=(px(10), px(3), px(10), 0))
    bar.grid(row=0, column=0, sticky='ew')
    menus = []
    active_menu = None
    previous_focus = None

    def dismiss(event=None):
        nonlocal active_menu
        if active_menu is not None:
            button, popup, _ = menus[active_menu]
            popup.grab_release()
            popup.place_forget()
            button.state(['!pressed'])
            active_menu = None
            if previous_focus is not None and previous_focus.winfo_exists():
                previous_focus.focus_set()
        return 'break'

    def invoke(command):
        dismiss()
        command()

    def move(delta):
        _, _, items = menus[active_menu]
        enabled = [item for item in items if item.instate(['!disabled'])]
        current = root.focus_get()
        index = enabled.index(current) if current in enabled else (-1 if delta > 0 else 0)
        enabled[(index + delta) % len(enabled)].focus_set()
        return 'break'

    def show(index, *, keyboard=False):
        nonlocal active_menu, previous_focus
        if index == active_menu:
            return dismiss()
        dismiss()
        button, popup, _ = menus[index]
        previous_focus = root.focus_get() or button
        active_menu = index
        button.state(['pressed'])
        popup.place(x=button.winfo_rootx() - root.winfo_rootx(),
                    y=button.winfo_rooty() - root.winfo_rooty() + button.winfo_height() + px(2))
        popup.lift()
        popup.update_idletasks()
        popup.grab_set()
        popup.focus_set()
        if keyboard:
            move(1)
        return 'break'

    def outside_click(event):
        popup = event.widget
        if 0 <= event.x < popup.winfo_width() and 0 <= event.y < popup.winfo_height():
            return
        target = root.winfo_containing(event.x_root, event.y_root)
        for index, (button, _, _) in enumerate(menus):
            if target == button:
                return show(index)
        return dismiss()

    def bind_navigation(widget, index):
        for key, delta in (('Up', -1), ('Down', 1), ('Shift-Tab', -1), ('Tab', 1)):
            widget.bind(f'<{key}>', lambda e, delta=delta: move(delta))
        for key, delta in (('Left', -1), ('Right', 1)):
            widget.bind(f'<{key}>', lambda e, delta=delta: show((index + delta) % len(menus), keyboard=True))
        widget.bind('<Escape>', dismiss)

    def invoke_item(event):
        event.widget.invoke()
        return 'break'

    def close_on_unmap(event):
        if event.widget == root:
            dismiss()

    root.bind('<Unmap>', close_on_unmap, add='+')
    root.bind('<Control-o>', lambda e: invoke(app.open_file))
    root.bind('<F1>', lambda e: invoke(app.show_guide))
    for index, (title, entries) in enumerate((
        ('File', [('Open file…  (Ctrl+O)', app.open_file), ('', None), ('Exit', app.close)]),
        ('Help', [('User Guide  (F1)', app.show_guide), ('About 12dAZ Converter', app.show_about),
                  ('', None), ('License and Third-party Notices', app.show_licenses)]),
    )):
        button = ttk.Button(bar, text=title, style='Menu.TButton', underline=0, width=0,
                            command=lambda index=index: show(index))
        button.pack(side='left', padx=(0, px(2)))
        button.bind('<Down>', lambda e, index=index: show(index, keyboard=True))
        button.bind('<Return>', lambda e, index=index: show(index, keyboard=True))
        root.bind(f'<Alt-{title[0].lower()}>', lambda e, index=index: show(index, keyboard=True))
        popup = ttk.Frame(root, style='Panel.TFrame', padding=px(5))
        for mouse_button in (1, 2, 3):
            popup.bind(f'<ButtonPress-{mouse_button}>', outside_click)
        bind_navigation(popup, index)
        items = []
        for text, command in entries:
            if command is None:
                ttk.Separator(popup).pack(fill='x', padx=px(4), pady=px(3))
                continue
            item = ttk.Button(popup, text=text, style='MenuItem.TButton',
                              command=lambda command=command: invoke(command))
            item.pack(fill='x')
            bind_navigation(item, index)
            item.bind('<Return>', invoke_item)
            items.append(item)
        menus.append((button, popup, items))
    app.open_menu_item = menus[0][2][0]


def build_layout(app):
    root = app.root
    scale = max(1, root.winfo_fpixels('1i') / 96)
    px = lambda n: round(n * scale)
    root.columnconfigure(0, weight=1)
    root.rowconfigure(2, weight=1)
    app.zoom_percent = tk.StringVar(value='100%')
    app.selection_status = tk.StringVar(value='No models selected')
    app.status_heading = tk.StringVar(value='Open a file to begin')
    app.warning_text = tk.StringVar(value='No warnings')

    build_menu_bar(app, px)
    root.bind('<Map>', lambda e: dark_titlebar(root) if e.widget == root else None, add='+')

    header = ttk.Frame(root, padding=(px(12), px(4), px(12), px(7)))
    header.grid(row=1, column=0, sticky='ew')
    ttk.Label(header, image=app.icons['logo']).pack(side='left', padx=(0, px(7)))
    ttk.Label(header, text='12dAZ Converter', style='Title.TLabel').pack(side='left')
    ttk.Separator(header, orient='vertical').pack(side='left', fill='y', padx=px(10), pady=px(6))
    app.open_button = ttk.Button(header, text='Open file', image=app.icons['folder'], compound='left',
                                  style='Open.TButton', command=app.open_file)
    app.open_button.pack(side='left', padx=(0, px(6)))
    file_box = ttk.Frame(header, style='File.TFrame', padding=px(3))
    file_box.pack(side='left', fill='both', expand=True)
    tk.Label(file_box, image=app.icons['file'], background=FIELD).pack(side='left', padx=(px(5), px(5)))
    tk.Label(file_box, textvariable=app.filename, background=FIELD, foreground=INK,
             font=('Segoe UI', 10), anchor='w', width=1).pack(side='left', fill='x', expand=True, padx=(0, px(5)))

    app.panes = ttk.Panedwindow(root, orient='horizontal')
    app.panes.grid(row=2, column=0, sticky='nsew', padx=px(12))

    def card(width=0):
        frame = ttk.Frame(app.panes, style='Panel.TFrame', padding=px(4), width=px(width))
        app.panes.add(frame, weight=0 if width else 1)
        return frame

    def card_heading(parent, text):
        bar = ttk.Frame(parent, style='Card.TFrame', padding=(px(6), 0), height=px(35))
        bar.pack(fill='x')
        bar.pack_propagate(False)
        ttk.Label(bar, text=text, style='Heading.TLabel').pack(side='left')
        ttk.Separator(parent).pack(fill='x', padx=px(4))
        return bar

    left = card(260)
    card_heading(left, 'Models')
    search_area = ttk.Frame(left, style='Card.TFrame', padding=(px(4), px(6), px(4), px(4)))
    search_area.pack(fill='x')
    search_box = ttk.Frame(search_area, style='Search.TFrame', padding=px(3))
    search_box.pack(fill='x')
    tk.Label(search_box, image=app.icons['search'], background=FIELD, borderwidth=0,
             padx=0, pady=0).pack(side='left', padx=(px(5), px(4)))
    app.search_entry = tk.Entry(search_box, textvariable=app.search, relief='flat', borderwidth=0,
                                background=FIELD, foreground=INK, insertbackground=INK,
                                highlightthickness=0, font=('Segoe UI', 10), width=16)
    app.search_entry.pack(side='left', fill='x', expand=True, padx=(0, px(5)), pady=px(3))
    app.search_placeholder = tk.Label(search_box, text='Search models…', background=FIELD, foreground='#93a0b3',
                                      font=('Segoe UI', 10), cursor='xterm', borderwidth=0, padx=0, pady=0)
    app.search_hint_x = px(12) + app.icons['search'].width()
    app.search_placeholder.place(x=app.search_hint_x, rely=.5, anchor='w')
    app.search_placeholder.bind('<Button-1>', lambda e: app.search_entry.focus_set())
    def search_focus(focused):
        search_box.state(['alternate' if focused else '!alternate'])
        app.refresh_search_hint()
    app.search_entry.bind('<FocusIn>', lambda e: search_focus(True))
    app.search_entry.bind('<FocusOut>', lambda e: search_focus(False))
    select_bar = ttk.Frame(left, style='Card.TFrame', padding=(px(4), 0, px(4), px(5)))
    select_bar.pack(fill='x')
    app.all_button = ttk.Button(select_bar, text='Select all', style='Small.TButton', command=lambda: app.select_all(True))
    app.all_button.pack(side='left')
    app.none_button = ttk.Button(select_bar, text='Clear', style='Small.TButton', command=lambda: app.select_all(False))
    app.none_button.pack(side='left', padx=px(4))
    tree_frame = ttk.Frame(left, style='Table.TFrame')
    tree_frame.pack(fill='both', expand=True, padx=px(4))
    app.tree = ttk.Treeview(tree_frame, columns=('model', 'count'), show='tree headings', selectmode='browse', height=12)
    app.tree.heading('#0', text='✓')
    app.tree.column('#0', width=px(30), minwidth=px(28), stretch=False)
    for key, title, width, anchor in [('model', 'Model', 160, 'w'), ('count', 'Items', 54, 'e')]:
        app.tree.heading(key, text=title, anchor=anchor)
        app.tree.column(key, width=px(width), minwidth=px(42), stretch=key == 'model', anchor=anchor)
    app.tree.tag_configure('even', background='#2b3038')
    app.tree.tag_configure('odd', background='#303640')
    app.tree.tag_configure('unsupported', foreground='#7d8899')
    scroll = ttk.Scrollbar(tree_frame, orient='vertical', command=app.tree.yview)
    app.tree.configure(yscrollcommand=scroll.set)
    app.tree.pack(side='left', fill='both', expand=True)
    scroll.pack(side='right', fill='y')
    app.tree.bind('<ButtonRelease-1>', app.toggle_model)
    app.tree.bind('<space>', app.toggle_model)
    ttk.Label(left, textvariable=app.summary, style='Muted.TLabel', wraplength=px(234),
              padding=(px(6), px(5), px(6), px(3))).pack(fill='x', side='bottom', before=tree_frame)

    middle = card()
    preview_bar = card_heading(middle, '2D preview')
    app.zoom_combo = ttk.Combobox(preview_bar, textvariable=app.zoom_percent, state='readonly', width=6,
                                  values=['25%', '50%', '100%', '150%', '200%', '400%'])
    app.zoom_combo.pack(side='right', padx=(px(5), 0))
    app.zoom_combo.bind('<<ComboboxSelected>>', lambda e: app.set_zoom_percent())
    for icon, text, command in [('zoom_out', '', lambda: app.zoom(.8)), ('zoom_in', '', lambda: app.zoom(1.25)), ('fit', 'Fit', app.fit)]:
        ttk.Button(preview_bar, text=text, image=app.icons[icon], compound='left', width=0,
                   style='Small.TButton', command=command).pack(side='right', padx=(px(3), 0))
    canvas_box = ttk.Frame(middle, style='Preview.TFrame', padding=px(3))
    canvas_box.pack(fill='both', expand=True, padx=px(3), pady=(px(4), 0))
    app.canvas = tk.Canvas(canvas_box, background=PREVIEW, highlightthickness=0, takefocus=True, cursor='crosshair')
    app.canvas.pack(fill='both', expand=True)
    app.canvas.bind('<Configure>', app.schedule_draw)
    app.canvas.bind('<MouseWheel>', lambda e: app.zoom(1.2 if e.delta > 0 else 1 / 1.2, e.x, e.y))
    app.canvas.bind('<ButtonPress-1>', lambda e: setattr(app, 'drag', (e.x, e.y)))
    app.canvas.bind('<B1-Motion>', app.pan)
    app.canvas.bind('<Motion>', app.mouse_coordinates)
    ttk.Label(middle, textvariable=app.cursor, style='Muted.TLabel', padding=(px(6), px(4))).pack(fill='x')

    right = card(280)
    card_heading(right, 'DXF export')
    export_body = ttk.Frame(right, style='Card.TFrame')
    export_body.pack(fill='x')
    settings_host = ttk.Frame(export_body, style='Card.TFrame')
    settings_view = tk.Canvas(settings_host, background=PANEL, highlightthickness=0, width=px(272), height=1)
    settings_scroll = ttk.Scrollbar(settings_host, orient='vertical', command=settings_view.yview)
    settings_view.configure(yscrollcommand=settings_scroll.set)
    settings_view.pack(side='left', fill='both', expand=True)
    settings = ttk.Frame(settings_view, style='Card.TFrame')
    settings_window = settings_view.create_window(0, 0, anchor='nw', window=settings)
    def resize_settings(event):
        settings_view.configure(height=event.height, scrollregion=(0, 0, event.width, event.height))
        update_settings_scrollbar()

    def resize_view(event):
        settings_view.itemconfigure(settings_window, width=event.width)
        update_settings_scrollbar()

    def update_settings_scrollbar():
        overflowing = settings.winfo_reqheight() > settings_view.winfo_height() + 2
        if overflowing and not settings_scroll.winfo_manager():
            settings_scroll.pack(side='right', fill='y')
        elif not overflowing and settings_scroll.winfo_manager():
            settings_scroll.pack_forget()

    settings.bind('<Configure>', resize_settings)
    settings_view.bind('<Configure>', resize_view)
    settings_view.bind('<MouseWheel>', lambda e: settings_view.yview_scroll(-1 if e.delta > 0 else 1, 'units'))
    form = ttk.Frame(settings, style='Card.TFrame', padding=(px(7), px(5)))
    form.pack(fill='x')
    form.columnconfigure(1, weight=1)
    def form_row(row, title, variable, values):
        ttk.Label(form, text=title, style='Card.TLabel').grid(row=row, column=0, sticky='w', padx=(0, px(6)), pady=px(3))
        combo = ttk.Combobox(form, textvariable=variable, state='readonly', width=13, values=values)
        combo.grid(row=row, column=1, sticky='ew', pady=px(3))
        return combo
    app.format_var = tk.StringVar(value='DXF 2018')
    app.layer_var = tk.StringVar(value='One per model')
    app.format_combo = form_row(0, 'DXF version', app.format_var, ['DXF 2018'])
    app.unit_combo = form_row(1, 'Units', app.units, ['metres', 'millimetres', 'feet', 'inches'])
    app.layer_combo = form_row(2, 'Layers', app.layer_var, ['One per model'])
    app.geometry_combo = form_row(3, 'Geometry', app.geometry_mode, ['Flatten 2D', 'Keep Z'])
    app.geometry_combo.bind('<<ComboboxSelected>>', app.geometry_changed)
    app.tolerance_entries = []
    for row, label, variable in ((4, 'Plan tolerance', app.plan_tolerance),
                                 (5, 'Elevation tolerance', app.elevation_tolerance)):
        ttk.Label(form, text=label, style='Card.TLabel').grid(row=row, column=0, sticky='w', pady=px(3))
        entry = ttk.Entry(form, textvariable=variable, width=13)
        entry.grid(row=row, column=1, sticky='ew', pady=px(3))
        app.tolerance_entries.append(entry)
    ttk.Label(form, text='Keep Z tolerances are in drawing units.', style='Muted.TLabel',
              wraplength=px(244)).grid(row=6, column=0, columnspan=2, sticky='w', pady=(px(4), 0))
    ttk.Label(form, text='Units are declared; coordinates stay unchanged.', style='Muted.TLabel',
              wraplength=px(244)).grid(row=7, column=0, columnspan=2, sticky='w', pady=(px(4), 0))
    app.linear_arc_checkbox = ttk.Checkbutton(form, text='Assume linear grade on arcs',
                                              variable=app.linear_arc_elevations, command=app.settings_changed)
    app.linear_arc_checkbox.grid(row=8, column=0, columnspan=2, sticky='w', pady=(px(5), 0))
    ttk.Separator(settings).pack(fill='x', padx=px(7), pady=(px(2), px(3)))
    destination = ttk.Frame(settings, style='Card.TFrame', padding=(px(7), px(4)))
    destination.pack(fill='x')
    ttk.Label(destination, text='Output file', style='Heading.TLabel').pack(anchor='w', pady=(0, px(5)))
    output_row = ttk.Frame(destination, style='Card.TFrame')
    output_row.pack(fill='x')
    app.browse_button = ttk.Button(output_row, text='…', width=2, style='Small.TButton', command=app.browse_output)
    app.browse_button.pack(side='right', fill='y', padx=(px(4), 0))
    app.output_entry = ttk.Entry(output_row, textvariable=app.output, width=16)
    app.output_entry.pack(side='left', fill='x', expand=True)

    advanced = ttk.Frame(settings, style='Card.TFrame', padding=(px(7), 0))
    advanced.pack(fill='x')
    app.advanced_button = ttk.Button(advanced, text='Input options ▸', style='Link.TButton', command=app.toggle_input_options)
    app.advanced_button.pack(anchor='w')
    app.input_options = ttk.Frame(advanced, style='Card.TFrame', padding=(0, px(3), 0, px(4)))
    ttk.Label(app.input_options, text='Encoding for the next file', style='Muted.TLabel').pack(anchor='w', pady=(0, px(3)))
    app.encoding_combo = ttk.Combobox(app.input_options, textvariable=app.encoding, state='readonly', width=25,
                values=['Automatic (BOM / UTF-8)', 'cp1252', 'utf-8', 'utf-16-le', 'utf-16-be'])
    app.encoding_combo.pack(fill='x')

    actions = ttk.Frame(export_body, style='Card.TFrame', padding=(px(7), px(6), px(7), px(3)))
    actions.pack(side='bottom', fill='x')
    app.export_button = ttk.Button(actions, text='Export DXF', image=app.icons['export'], compound='left',
                                    style='Primary.TButton', command=app.export, state='disabled')
    app.export_button.pack(fill='x')
    ttk.Label(actions, text='Unsupported content is listed in the report.', style='Muted.TLabel',
              wraplength=px(244)).pack(anchor='w', pady=(px(4), 0))
    settings_host.pack(fill='both', expand=True)

    app.progress = ttk.Progressbar(root, mode='indeterminate')
    app.progress.grid(row=3, column=0, sticky='ew', padx=px(12), pady=px(2))
    footer_content = ttk.Frame(root, style='Panel.TFrame', padding=(px(9), px(4)))
    footer_content.grid(row=4, column=0, sticky='ew', padx=px(12))
    app.state_icon = ttk.Label(footer_content, image=app.icons['info'], style='Card.TLabel')
    app.state_icon.pack(side='left', padx=(0, px(5)))
    ttk.Label(footer_content, textvariable=app.status_heading, style='Status.TLabel').pack(side='left')
    ttk.Separator(footer_content, orient='vertical').pack(side='left', fill='y', padx=px(8), pady=px(3))
    ttk.Label(footer_content, textvariable=app.selection_status, style='Card.TLabel').pack(side='left')
    app.report_button = ttk.Button(footer_content, text='View report', style='Link.TButton', command=app.show_report, state='disabled')
    app.report_button.pack(side='right', padx=(px(4), 0))
    app.warning_button = ttk.Button(footer_content, textvariable=app.warning_text, image=app.icons['warning'],
                                     compound='left', style='Warning.TButton', command=app.show_report)
    app.warning_button.pack(side='right')
    app.folder_button = ttk.Button(footer_content, text='Open output folder', style='Small.TButton', command=app.open_folder, state='disabled')
    app.folder_button.pack(side='right', padx=px(4))
    app.cancel_button = ttk.Button(footer_content, text='Cancel', style='Small.TButton', command=app.cancel, state='disabled')
    app.cancel_button.pack(side='right', padx=(px(4), 0))
    bottom = ttk.Frame(root, padding=(px(15), px(3), px(15), px(4)))
    bottom.grid(row=5, column=0, sticky='ew')
    ttk.Label(bottom, text='Local files  ·  Offline conversion', foreground=MUTED, font=('Segoe UI', 9)).pack(side='left')
    ttk.Label(bottom, text=f'v{__version__}', foreground=MUTED, font=('Segoe UI', 9)).pack(side='right')
    app.status_detail = ttk.Label(bottom, textvariable=app.status, foreground=MUTED, font=('Segoe UI', 9), anchor='e', width=1)
    app.status_detail.pack(side='right', fill='x', expand=True, padx=px(8))
    app.update_status_display()
