"""Desktop presentation matching the three-panel design proposal."""
import tkinter as tk
from tkinter import ttk
import sys

from . import __version__
from .icons import make_icon

BG = '#24272d'
PANEL = '#2e3239'
FIELD = '#252a31'
BORDER = '#4a515e'
INK = '#e9edf4'
MUTED = '#a5afbe'
BLUE = '#4a97ff'
PRIMARY = '#2477dc'


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


def configure_styles(app):
    root = app.root
    style = ttk.Style(root)
    style.theme_use('clam')
    style.configure('.', font=('Segoe UI', 10), background=BG, foreground=INK)
    style.configure('TFrame', background=BG)
    style.configure('Card.TFrame', background=PANEL)
    style.configure('White.TFrame', background=FIELD)
    style.configure('TLabel', background=BG, foreground=INK)
    style.configure('Card.TLabel', background=PANEL)
    style.configure('Muted.TLabel', background=PANEL, foreground=MUTED, font=('Segoe UI', 9))
    style.configure('Title.TLabel', font=('Segoe UI', 21, 'bold'))
    style.configure('Heading.TLabel', background=PANEL, font=('Segoe UI', 12, 'bold'))
    style.configure('Status.TLabel', background=PANEL, font=('Segoe UI', 11, 'bold'))
    style.configure('TButton', padding=(12, 8), relief='flat', borderwidth=1,
                    background='#383f4a', bordercolor='#4a5564', lightcolor='#383f4a', darkcolor='#383f4a')
    style.map('TButton', background=[('disabled', '#30353d'), ('pressed', '#314a6c'), ('active', '#414c5c')],
              foreground=[('disabled', '#778394')], bordercolor=[('focus', BLUE), ('active', '#719fd9')])
    style.configure('Open.TButton', font=('Segoe UI', 11, 'bold'), padding=(16, 9))
    style.configure('Small.TButton', padding=(8, 5), font=('Segoe UI', 9))
    style.configure('Link.TButton', padding=(4, 5), borderwidth=0, background=PANEL,
                    foreground=BLUE, font=('Segoe UI', 10, 'underline'))
    style.map('Link.TButton', background=[('active', '#39485d')], foreground=[('disabled', '#778394')])
    style.configure('Warning.TButton', padding=(8, 5), borderwidth=0, background=PANEL,
                    foreground='#f3bb60', font=('Segoe UI', 10, 'bold'))
    style.map('Warning.TButton', background=[('active', '#514330')], foreground=[('disabled', '#778394')])
    style.configure('Primary.TButton', foreground='white', background=PRIMARY, bordercolor=PRIMARY,
                    lightcolor=PRIMARY, darkcolor=PRIMARY, font=('Segoe UI', 14, 'bold'), padding=(20, 15))
    style.map('Primary.TButton', background=[('disabled', '#405778'), ('pressed', '#1769ca'), ('active', '#2c7de7')],
              foreground=[('disabled', '#b9c9e0'), ('!disabled', 'white')],
              bordercolor=[('disabled', '#405778'), ('!disabled', PRIMARY)])
    style.configure('TEntry', padding=8, fieldbackground=FIELD, bordercolor=BORDER, lightcolor=BORDER,
                    darkcolor=BORDER, insertcolor=INK, relief='flat')
    style.map('TEntry', bordercolor=[('focus', BLUE)], lightcolor=[('focus', BLUE)], darkcolor=[('focus', BLUE)])
    style.configure('TCombobox', padding=7, fieldbackground=FIELD, background=FIELD, arrowsize=13,
                    bordercolor=BORDER, lightcolor=BORDER, darkcolor=BORDER, arrowcolor='#bec9d9')
    style.map('TCombobox', fieldbackground=[('readonly', FIELD), ('disabled', '#30353d')],
              foreground=[('readonly', INK), ('disabled', '#93a0b3')], selectbackground=[('readonly', FIELD)],
              selectforeground=[('readonly', INK)], bordercolor=[('focus', BLUE)])
    row_height = round(35 * max(1, root.winfo_fpixels('1i') / 96))
    style.configure('Treeview', background=FIELD, fieldbackground=FIELD, borderwidth=0, rowheight=row_height,
                    foreground=INK, font=('Segoe UI', 10), lightcolor=BORDER, darkcolor=BORDER)
    style.map('Treeview', background=[('selected', '#294f7c')], foreground=[('selected', '#f0f5fc')])
    style.configure('Treeview.Heading', background='#393f49', foreground='#dce5f2', padding=(8, 9),
                    font=('Segoe UI', 10, 'bold'), relief='flat', borderwidth=0)
    style.map('Treeview.Heading', background=[('active', '#444e5e')])
    style.configure('TPanedwindow', background=BG)
    style.configure('Horizontal.TProgressbar', troughcolor=BG, background=BLUE, borderwidth=0, thickness=3)
    style.configure('TScrollbar', background='#596371', troughcolor=PANEL, bordercolor=BORDER,
                    lightcolor=PANEL, darkcolor=PANEL, arrowcolor=INK, borderwidth=0, gripcount=0)
    style.configure('Vertical.TScrollbar', background='#596371', troughcolor=PANEL, bordercolor=BORDER,
                    lightcolor=PANEL, darkcolor=PANEL, arrowcolor=INK, borderwidth=0, arrowsize=12)
    root.configure(background=BG)
    style.configure('TSeparator', background=BORDER)
    style.configure('Menu.TMenubutton', background=BG, foreground=MUTED, padding=(10, 5), font=('Segoe UI', 9))
    style.layout('Menu.TMenubutton', [('Menubutton.padding', {'sticky': 'nswe', 'children': [
        ('Menubutton.label', {'sticky': 'nswe'})]})])
    style.map('Menu.TMenubutton', background=[('active', '#3c4654')], foreground=[('active', INK)])
    root.option_add('*TCombobox*Listbox.background', FIELD)
    root.option_add('*TCombobox*Listbox.foreground', INK)
    root.option_add('*TCombobox*Listbox.selectBackground', '#294f7c')
    root.option_add('*TCombobox*Listbox.selectForeground', INK)
    app.icons = {}
    ratio = max(1, root.winfo_fpixels('1i') / 96)
    for name, colour in [('folder', BLUE), ('file', '#b4c1d3'), ('search', '#a3b2c7'), ('fit', '#cad4e3'),
                         ('zoom_in', '#cad4e3'), ('zoom_out', '#cad4e3'), ('export', '#ffffff'),
                         ('checked', BLUE), ('empty', '#929eae'), ('warning', '#e5ab43'),
                         ('success', '#40af71'), ('info', '#a2afc2')]:
        app.icons[name] = make_icon(root, name, colour, round(21 * ratio))
    app.icons['logo'] = make_icon(root, 'logo', BLUE, round(38 * ratio))


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

    menu_bar = ttk.Frame(root, padding=(px(12), 0))
    menu_bar.grid(row=0, column=0, sticky='ew')
    app.file_menu_button = ttk.Menubutton(menu_bar, text='File', menu=app.file_menu, style='Menu.TMenubutton', underline=0, width=0)
    app.file_menu_button.pack(side='left')
    app.help_menu_button = ttk.Menubutton(menu_bar, text='Help', menu=app.help_menu, style='Menu.TMenubutton', underline=0, width=0)
    app.help_menu_button.pack(side='left')
    def post_menu(menu, button):
        menu.tk_popup(button.winfo_rootx(), button.winfo_rooty() + button.winfo_height())
    root.bind('<Alt-f>', lambda e: post_menu(app.file_menu, app.file_menu_button))
    root.bind('<Alt-h>', lambda e: post_menu(app.help_menu, app.help_menu_button))
    root.bind('<Map>', lambda e: dark_titlebar(root) if e.widget == root else None, add='+')

    header = ttk.Frame(root, padding=(px(18), px(12), px(18), px(10)))
    header.grid(row=1, column=0, sticky='ew')
    ttk.Label(header, image=app.icons['logo']).pack(side='left', padx=(0, px(12)))
    ttk.Label(header, text='12dAZ Converter', style='Title.TLabel').pack(side='left')
    ttk.Separator(header, orient='vertical').pack(side='left', fill='y', padx=px(20), pady=px(5))
    app.open_button = ttk.Button(header, text='Open file', image=app.icons['folder'], compound='left',
                                  style='Open.TButton', command=app.open_file)
    app.open_button.pack(side='left', padx=(0, px(14)))
    file_box = tk.Frame(header, background='#292e36', highlightthickness=1, highlightbackground=BORDER)
    file_box.pack(side='left', fill='both', expand=True)
    tk.Label(file_box, image=app.icons['file'], background='#292e36').pack(side='left', padx=(px(14), px(10)))
    tk.Label(file_box, textvariable=app.filename, background='#292e36', foreground='#e1e8f2',
             font=('Segoe UI', 11), anchor='w', width=1).pack(side='left', fill='x', expand=True, pady=px(9))

    app.panes = ttk.Panedwindow(root, orient='horizontal')
    app.panes.grid(row=2, column=0, sticky='nsew', padx=px(18))

    def card(width=0):
        frame = tk.Frame(app.panes, background=PANEL, highlightthickness=1, highlightbackground=BORDER, width=px(width))
        app.panes.add(frame, weight=0 if width else 1)
        return frame

    def card_heading(parent, text):
        bar = ttk.Frame(parent, style='Card.TFrame', padding=(px(14), px(12)))
        bar.pack(fill='x')
        ttk.Label(bar, text=text, style='Heading.TLabel').pack(side='left')
        ttk.Separator(parent).pack(fill='x')
        return bar

    left = card(290)
    card_heading(left, 'Models')
    search_area = ttk.Frame(left, style='Card.TFrame', padding=(px(10), px(12), px(10), px(8)))
    search_area.pack(fill='x')
    search_box = tk.Frame(search_area, background=FIELD, highlightthickness=1, highlightbackground=BORDER)
    search_box.pack(fill='x')
    tk.Label(search_box, image=app.icons['search'], background=FIELD).pack(side='left', padx=(px(9), px(3)))
    app.search_entry = tk.Entry(search_box, textvariable=app.search, relief='flat', borderwidth=0,
                                background=FIELD, foreground=INK, font=('Segoe UI', 10), width=16)
    app.search_entry.pack(side='left', fill='x', expand=True, padx=(0, px(8)), pady=px(9))
    app.search_placeholder = tk.Label(search_box, text='Search models…', background=FIELD, foreground='#93a0b3',
                                      font=('Segoe UI', 10), cursor='xterm')
    app.search_placeholder.place(x=px(37), rely=.5, anchor='w')
    app.search_placeholder.bind('<Button-1>', lambda e: app.search_entry.focus_set())
    app.search_entry.bind('<FocusIn>', lambda e: app.search_placeholder.place_forget())
    app.search_entry.bind('<FocusOut>', lambda e: app.refresh_search_hint())
    select_bar = ttk.Frame(left, style='Card.TFrame', padding=(px(10), 0, px(10), px(7)))
    select_bar.pack(fill='x')
    app.all_button = ttk.Button(select_bar, text='Select all', style='Small.TButton', command=lambda: app.select_all(True))
    app.all_button.pack(side='left')
    app.none_button = ttk.Button(select_bar, text='Clear', style='Small.TButton', command=lambda: app.select_all(False))
    app.none_button.pack(side='left', padx=px(6))
    tree_frame = ttk.Frame(left, style='White.TFrame')
    tree_frame.pack(fill='both', expand=True, padx=px(8), pady=(0, px(8)))
    app.tree = ttk.Treeview(tree_frame, columns=('model', 'count'), show='tree headings', selectmode='browse', height=12)
    app.tree.heading('#0', text='✓')
    app.tree.column('#0', width=px(36), minwidth=px(32), stretch=False)
    for key, title, width, anchor in [('model', 'Model', 180, 'w'), ('count', 'Items', 62, 'e')]:
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
    ttk.Label(left, textvariable=app.summary, style='Muted.TLabel', wraplength=px(260),
              padding=(px(12), px(6), px(12), px(12))).pack(fill='x', side='bottom', before=tree_frame)

    middle = card()
    preview_bar = card_heading(middle, '2D preview')
    app.zoom_combo = ttk.Combobox(preview_bar, textvariable=app.zoom_percent, state='readonly', width=6,
                                  values=['25%', '50%', '100%', '150%', '200%', '400%'])
    app.zoom_combo.pack(side='right', padx=(px(8), 0))
    app.zoom_combo.bind('<<ComboboxSelected>>', lambda e: app.set_zoom_percent())
    for icon, text, command in [('zoom_out', '', lambda: app.zoom(.8)), ('zoom_in', '', lambda: app.zoom(1.25)), ('fit', 'Fit', app.fit)]:
        ttk.Button(preview_bar, text=text, image=app.icons[icon], compound='left', width=0,
                   style='Small.TButton', command=command).pack(side='right', padx=(px(4), 0))
    canvas_box = tk.Frame(middle, background='#171c22')
    canvas_box.pack(fill='both', expand=True, padx=px(6), pady=(px(6), 0))
    app.canvas = tk.Canvas(canvas_box, background='#171c22', highlightthickness=0, takefocus=True, cursor='crosshair')
    app.canvas.pack(fill='both', expand=True)
    app.canvas.bind('<Configure>', app.schedule_draw)
    app.canvas.bind('<MouseWheel>', lambda e: app.zoom(1.2 if e.delta > 0 else 1 / 1.2, e.x, e.y))
    app.canvas.bind('<ButtonPress-1>', lambda e: setattr(app, 'drag', (e.x, e.y)))
    app.canvas.bind('<B1-Motion>', app.pan)
    app.canvas.bind('<Motion>', app.mouse_coordinates)
    ttk.Label(middle, textvariable=app.cursor, style='Muted.TLabel', padding=(px(12), px(7))).pack(fill='x')

    right = card(290)
    card_heading(right, 'DXF export')
    settings_host = ttk.Frame(right, style='Card.TFrame')
    settings_view = tk.Canvas(settings_host, background=PANEL, highlightthickness=0, width=px(285))
    settings_scroll = ttk.Scrollbar(settings_host, orient='vertical', command=settings_view.yview)
    settings_view.configure(yscrollcommand=settings_scroll.set)
    settings_view.pack(side='left', fill='both', expand=True)
    settings = ttk.Frame(settings_view, style='Card.TFrame')
    settings_window = settings_view.create_window(0, 0, anchor='nw', window=settings)
    def size_settings(event=None):
        settings_view.itemconfigure(settings_window, width=settings_view.winfo_width())
        settings_view.configure(scrollregion=settings_view.bbox('all'))
        if settings.winfo_reqheight() > settings_view.winfo_height() + 2:
            settings_scroll.pack(side='right', fill='y')
        else:
            settings_scroll.pack_forget()
    settings.bind('<Configure>', size_settings)
    settings_view.bind('<Configure>', size_settings)
    settings_view.bind('<MouseWheel>', lambda e: settings_view.yview_scroll(-1 if e.delta > 0 else 1, 'units'))
    form = ttk.Frame(settings, style='Card.TFrame', padding=(px(16), px(12)))
    form.pack(fill='x')
    form.columnconfigure(1, weight=1)
    def form_row(row, title, variable, values):
        ttk.Label(form, text=title, style='Card.TLabel').grid(row=row, column=0, sticky='w', padx=(0, px(14)), pady=px(6))
        combo = ttk.Combobox(form, textvariable=variable, state='readonly', width=16, values=values)
        combo.grid(row=row, column=1, sticky='ew', pady=px(6))
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
        ttk.Label(form, text=label, style='Card.TLabel').grid(row=row, column=0, sticky='w', pady=px(6))
        entry = ttk.Entry(form, textvariable=variable, width=16)
        entry.grid(row=row, column=1, sticky='ew', pady=px(6))
        app.tolerance_entries.append(entry)
    ttk.Label(form, text='Keep Z tolerances are in drawing units.', style='Muted.TLabel',
              wraplength=px(260)).grid(row=6, column=0, columnspan=2, sticky='w', pady=(px(7), 0))
    ttk.Label(form, text='Units are declared; coordinates stay unchanged.', style='Muted.TLabel',
              wraplength=px(260)).grid(row=7, column=0, columnspan=2, sticky='w', pady=(px(7), 0))
    app.linear_arc_checkbox = ttk.Checkbutton(form, text='Assume linear grade on arcs',
                                              variable=app.linear_arc_elevations, command=app.settings_changed)
    app.linear_arc_checkbox.grid(row=8, column=0, columnspan=2, sticky='w', pady=px(6))
    ttk.Separator(settings).pack(fill='x', padx=px(16), pady=px(5))
    destination = ttk.Frame(settings, style='Card.TFrame', padding=(px(16), px(10)))
    destination.pack(fill='x')
    ttk.Label(destination, text='Output file', style='Heading.TLabel').pack(anchor='w', pady=(0, px(10)))
    output_row = ttk.Frame(destination, style='Card.TFrame')
    output_row.pack(fill='x')
    app.browse_button = ttk.Button(output_row, text='…', width=2, command=app.browse_output)
    app.browse_button.pack(side='right', padx=(px(6), 0))
    app.output_entry = ttk.Entry(output_row, textvariable=app.output, width=16)
    app.output_entry.pack(side='left', fill='x', expand=True)

    advanced = ttk.Frame(settings, style='Card.TFrame', padding=(px(16), 0))
    advanced.pack(fill='x')
    app.advanced_button = ttk.Button(advanced, text='Input options ▸', style='Link.TButton', command=app.toggle_input_options)
    app.advanced_button.pack(anchor='w')
    app.input_options = ttk.Frame(advanced, style='Card.TFrame', padding=(0, px(5), 0, px(8)))
    ttk.Label(app.input_options, text='Encoding for the next file', style='Muted.TLabel').pack(anchor='w', pady=(0, px(5)))
    app.encoding_combo = ttk.Combobox(app.input_options, textvariable=app.encoding, state='readonly', width=25,
                values=['Automatic (BOM / UTF-8)', 'cp1252', 'utf-8', 'utf-16-le', 'utf-16-be'])
    app.encoding_combo.pack(fill='x')

    actions = ttk.Frame(right, style='Card.TFrame', padding=(px(16), px(14)))
    actions.pack(side='bottom', fill='x')
    app.export_button = ttk.Button(actions, text='Export DXF', image=app.icons['export'], compound='left',
                                    style='Primary.TButton', command=app.export, state='disabled')
    app.export_button.pack(fill='x')
    ttk.Label(actions, text='Unsupported content is listed in the report.', style='Muted.TLabel',
              wraplength=px(260)).pack(anchor='w', pady=(px(10), 0))
    settings_host.pack(fill='both', expand=True)

    app.progress = ttk.Progressbar(root, mode='indeterminate')
    app.progress.grid(row=3, column=0, sticky='ew', padx=px(18), pady=(px(8), px(5)))
    footer = tk.Frame(root, background=PANEL, highlightthickness=1, highlightbackground=BORDER)
    footer.grid(row=4, column=0, sticky='ew', padx=px(18), pady=(px(10), 0))
    footer_content = ttk.Frame(footer, style='Card.TFrame', padding=(px(12), px(8)))
    footer_content.pack(fill='x')
    app.state_icon = ttk.Label(footer_content, image=app.icons['info'], style='Card.TLabel')
    app.state_icon.pack(side='left', padx=(0, px(8)))
    ttk.Label(footer_content, textvariable=app.status_heading, style='Status.TLabel').pack(side='left')
    ttk.Separator(footer_content, orient='vertical').pack(side='left', fill='y', padx=px(14), pady=px(4))
    ttk.Label(footer_content, textvariable=app.selection_status, style='Card.TLabel').pack(side='left')
    app.report_button = ttk.Button(footer_content, text='View report', style='Link.TButton', command=app.show_report, state='disabled')
    app.report_button.pack(side='right', padx=(px(12), 0))
    app.warning_button = ttk.Button(footer_content, textvariable=app.warning_text, image=app.icons['warning'],
                                     compound='left', style='Warning.TButton', command=app.show_report)
    app.warning_button.pack(side='right')
    app.folder_button = ttk.Button(footer_content, text='Open output folder', style='Small.TButton', command=app.open_folder, state='disabled')
    app.folder_button.pack(side='right', padx=px(10))
    app.cancel_button = ttk.Button(footer_content, text='Cancel', style='Small.TButton', command=app.cancel, state='disabled')
    app.cancel_button.pack(side='right', padx=px(6))
    bottom = ttk.Frame(root, padding=(px(20), px(7), px(20), px(8)))
    bottom.grid(row=5, column=0, sticky='ew')
    ttk.Label(bottom, text='Local files  ·  Offline conversion', foreground=MUTED, font=('Segoe UI', 9)).pack(side='left')
    ttk.Label(bottom, text=f'v{__version__}', foreground=MUTED, font=('Segoe UI', 9)).pack(side='right')
    app.status_detail = ttk.Label(bottom, textvariable=app.status, foreground=MUTED, font=('Segoe UI', 9), anchor='e', width=1)
    app.status_detail.pack(side='right', fill='x', expand=True, padx=px(20))
    app.update_status_display()
