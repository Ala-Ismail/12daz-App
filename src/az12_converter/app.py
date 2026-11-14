"""Offline Tk desktop application. All input and CAD work runs in a process."""
from dataclasses import asdict
from importlib.resources import files
import json
import math
import multiprocessing as mp
import os
from pathlib import Path
import queue
import subprocess
import sys
import tkinter as tk
from tkinter import ttk, filedialog, messagebox

from . import __version__, COPYRIGHT
from .service import Options
from .worker import run_job


PALETTE = {"red": "#ff7474", "green": "#68e35c", "cyan": "#76d9fa", "blue": "#829dff", "yellow": "#ffdb78", "magenta": "#e9a1ff", "white": "#e5edf6", "black": "#e5edf6", "grey": "#9baabb", "gray": "#9baabb"}


def resource_text(name):
    return files("az12_converter").joinpath("resources", name).read_text(encoding="utf-8-sig")


class ConverterApp:
    def __init__(self, root):
        self.root = root
        root.title("12dAZ Converter")
        root.geometry("1440x900")
        root.minsize(1040, 680)
        self.context = mp.get_context("spawn")
        self.process = self.messages = self.cancel_event = None
        self.job_id = 0
        self.busy = False
        self.closing = False
        self.scan = self.result = None
        self.path = None
        self.selected = set()
        self.model_names = {}
        self.scale = 1.
        self.fit_scale = 1.
        self.preview_pending = False
        self.preview_revision = 0
        self._preview_after = None
        self.origin = (0., 0.)
        self.offset = [0., 0.]
        self.drag = None
        self._redraw_job = None
        self.filename = tk.StringVar(value="Open a local .12daz or .12da file to begin")
        self.search = tk.StringVar()
        self.output = tk.StringVar()
        self.units = tk.StringVar(value="Choose units…")
        self.encoding = tk.StringVar(value="Automatic (BOM / UTF-8)")
        self.status = tk.StringVar(value="Ready · all processing stays on this computer")
        self.summary = tk.StringVar(value="No file loaded")
        self.cursor = tk.StringVar(value="Plan view · Z = 0")
        self._styles()
        root.iconphoto(True, self.icons['logo'])
        self._menu()
        self._layout()
        self.search.trace_add("write", lambda *_: self.populate_models())
        self.units.trace_add("write", lambda *_: self.settings_changed())
        self.output.trace_add("write", lambda *_: self.settings_changed())
        self.status.trace_add("write", lambda *_: self.update_status_display())
        self.update_controls()
        root.protocol("WM_DELETE_WINDOW", self.close)
        root.bind("<Control-o>", lambda e: self.open_file())
        root.bind("<F1>", lambda e: self.show_guide())
        root.after(100, self.poll)

    def _styles(self):
        from .ui_layout import configure_styles
        configure_styles(self)

    def _menu(self):
        from .ui_layout import PANEL, INK
        menu_style = {'tearoff': False, 'background': PANEL, 'foreground': INK,
                      'activebackground': '#3c506d', 'activeforeground': '#ffffff',
                      'borderwidth': 1, 'activeborderwidth': 0}
        self.file_menu = tk.Menu(self.root, **menu_style)
        self.file_menu.add_command(label="Open file…", accelerator="Ctrl+O", command=self.open_file)
        self.file_menu.add_separator()
        self.file_menu.add_command(label="Exit", command=self.close)
        help_menu = tk.Menu(self.root, **menu_style)
        help_menu.add_command(label="User Guide", accelerator="F1", command=self.show_guide)
        help_menu.add_command(label="About 12dAZ Converter", command=self.show_about)
        help_menu.add_separator()
        help_menu.add_command(label="License and Third-party Notices", command=self.show_licenses)
        self.help_menu = help_menu

    def _layout(self):
        from .ui_layout import build_layout
        build_layout(self)

    def refresh_search_hint(self):
        if self.search.get() or self.root.focus_get() == self.search_entry:
            self.search_placeholder.place_forget()
        else:
            self.search_placeholder.place(x=round(37 * max(1, self.root.winfo_fpixels('1i') / 96)), rely=.5, anchor='w')

    def toggle_input_options(self):
        if self.input_options.winfo_manager():
            self.input_options.pack_forget()
            self.advanced_button.configure(text='Input options ▸')
        else:
            self.input_options.pack(fill='x')
            self.advanced_button.configure(text='Input options ▾')

    def update_status_display(self):
        text = self.status.get()
        if self.busy:
            headline = 'Updating preview' if self.action == 'preview' else ('Reading archive' if self.action == 'inspect' else 'Exporting DXF')
            icon = 'info'
        elif self.preview_pending:
            headline, icon = 'Updating preview', 'info'
        elif text.startswith('Failed'):
            headline, icon = 'Could not complete', 'warning'
        elif text.startswith('Cancelled') or 'cancelled' in text.lower():
            headline, icon = 'Cancelled', 'info'
        elif self.result and self.result.get('published'):
            headline, icon = 'DXF exported', 'success'
        elif self.scan and not self.selected:
            headline, icon = 'Select models to export', 'info'
        elif self.scan and self.units.get() not in {'metres', 'millimetres', 'feet', 'inches'}:
            headline, icon = 'Choose drawing units', 'info'
        elif self.scan:
            headline, icon = 'Ready to export', 'success'
        else:
            headline, icon = 'Open a file to begin', 'info'
        self.status_heading.set(headline)
        self.state_icon.configure(image=self.icons[icon])
        self.selection_status.set(f'{len(self.selected)} models selected' if self.scan else 'No models selected')
        report = self.result or self.scan
        warnings = report['diagnostics']['total'] if report else 0
        self.warning_text.set(f'{warnings:,} warnings' if warnings else 'No warnings')
        if warnings:
            self.warning_button.pack(side='right', before=self.report_button)
        else:
            self.warning_button.pack_forget()
        if self.busy:
            self.cancel_button.pack(side='right', padx=6)
            self.progress.grid()
        else:
            self.cancel_button.pack_forget()
            self.progress.grid_remove()
        if self.result and self.result.get('published'):
            self.folder_button.pack(side='right', padx=10)
        else:
            self.folder_button.pack_forget()

    def set_zoom_percent(self):
        try:
            target = float(self.zoom_percent.get().rstrip('%')) / 100 * self.fit_scale
            self.zoom(target / self.scale)
        except ValueError:
            self.zoom_percent.set(f'{self.scale / self.fit_scale * 100:.0f}%')

    def settings_changed(self):
        self.result = None
        self.folder_button.configure(state="disabled")
        if self.scan and not self.busy and not self.preview_pending:
            self.status.set("Ready · review selected models and units before exporting")
        self.output_entry.xview_moveto(1)
        self.update_controls()

    def update_controls(self):
        can_export = self.scan and self.selected and self.units.get() in {'metres', 'millimetres', 'feet', 'inches'} and self.output.get().lower().endswith('.dxf') and Path(self.output.get()).parent.is_dir()
        self.export_button.configure(state='normal' if can_export and not self.busy and not self.preview_pending else 'disabled')
        self.cancel_button.configure(state='normal' if self.busy else 'disabled')
        preview_running = self.busy and self.action == 'preview'
        for widget in (self.open_button, self.search_entry, self.output_entry, self.browse_button, self.advanced_button):
            widget.configure(state='disabled' if self.busy else 'normal')
        for widget in (self.all_button, self.none_button):
            widget.configure(state='normal' if not self.busy or preview_running else 'disabled')
        for widget in (self.unit_combo, self.encoding_combo, self.format_combo, self.layer_combo):
            widget.configure(state='disabled' if self.busy else 'readonly')
        self.file_menu.entryconfigure(0, state='disabled' if self.busy else 'normal')
        self.report_button.configure(state='normal' if self.result or self.scan else 'disabled')
        self.update_status_display()

    def open_file(self):
        if self.busy:
            return
        path = filedialog.askopenfilename(parent=self.root, title="Open 12d archive", filetypes=[("12d archives", "*.12daz *.12da"), ("All files", "*.*")])
        if path:
            self.load_path(path)

    def load_path(self, path):
        if self.busy:
            return
        if self._preview_after:
            self.root.after_cancel(self._preview_after)
            self._preview_after = None
        self.preview_pending = False
        self.preview_revision += 1
        self.path = path
        self.scan = self.result = None
        self.selected.clear()
        self.units.set("Choose units…")
        self.output.set(str(Path(path).with_name(Path(path).stem + "_2D.dxf")))
        self.filename.set(Path(path).name)
        self.summary.set("Reading models…")
        self.populate_models()
        self.draw()
        self.start_job("inspect", Options(encoding=self.chosen_encoding()))

    def chosen_encoding(self):
        return None if self.encoding.get().startswith("Automatic") else self.encoding.get()

    def populate_models(self):
        self.refresh_search_hint()
        focused = self.tree.focus()
        position = self.tree.yview()[0] if self.tree.get_children() else 0
        self.tree.delete(*self.tree.get_children())
        self.model_names.clear()
        if not self.scan:
            self.update_status_display()
            return
        query = self.search.get().casefold()
        visible = 0
        for i, model in enumerate(self.scan['models']):
            if query not in model['name'].casefold():
                continue
            iid, name = str(i), model['name']
            self.model_names[iid] = name
            marker = 'checked' if name in self.selected else 'empty'
            tags = ['even' if visible % 2 == 0 else 'odd']
            if not model['convertible']:
                tags.append('unsupported')
            self.tree.insert('', 'end', iid=iid, image=self.icons[marker], values=(name, f"{model['count']:,}"), tags=tags)
            visible += 1
        if focused in self.model_names:
            self.tree.focus(focused)
            self.tree.selection_set(focused)
        self.tree.yview_moveto(position)
        total = sum(m['count'] for m in self.scan['models'] if m['name'] in self.selected)
        self.summary.set(f'{total:,} source objects selected\nClick a row or press Space to toggle.')
        self.update_status_display()

    def toggle_model(self, event):
        if (self.busy and self.action != "preview") or not self.scan:
            return
        row = self.tree.focus() if event.keysym == "space" else self.tree.identify_row(event.y)
        if row in self.model_names:
            name = self.model_names[row]
            if name in self.selected:
                self.selected.remove(name)
            else:
                model = next(m for m in self.scan["models"] if m["name"] == name)
                if model["convertible"]:
                    self.selected.add(name)
            self.populate_models()
            self.settings_changed()
            self.fit()
            self.request_selected_preview()

    def select_all(self, enabled):
        if (self.busy and self.action != "preview") or not self.scan:
            return
        self.selected = {m["name"] for m in self.scan["models"] if m["convertible"]} if enabled else set()
        self.populate_models()
        self.settings_changed()
        self.fit()
        self.request_selected_preview()

    def browse_output(self):
        path = filedialog.asksaveasfilename(parent=self.root, title="Save 2D DXF", defaultextension=".dxf", filetypes=[("DXF drawings", "*.dxf")], initialfile=Path(self.output.get()).name)
        if path:
            self.output.set(path)

    def request_selected_preview(self):
        """Debounce selection changes; only the latest selection may replace the view."""
        self.preview_revision += 1
        if self._preview_after:
            self.root.after_cancel(self._preview_after)
            self._preview_after = None
        if self.busy and self.action == 'preview':
            self.cancel_event.set()
        cached_models = self.scan.get('preview_selected_models', sorted(m['name'] for m in self.scan['models'] if m['convertible']))
        if not self.selected or (not self.busy and sorted(self.selected) == cached_models):
            self.preview_pending = False
            self.update_controls()
            return
        self.preview_pending = True
        self.status.set('Updating preview for selected models…')
        self._preview_after = self.root.after(300, self.refresh_selected_preview)
        self.update_controls()

    def refresh_selected_preview(self):
        self._preview_after = None
        if self.busy:
            if self.action == 'preview':
                self.cancel_event.set()
            return  # finish() starts the pending selection after safe cleanup.
        if not self.selected or self.closing:
            self.preview_pending = False
            self.update_controls()
            return
        self.preview_pending = False
        self.preview_job_revision = self.preview_revision
        self.start_job('preview', Options(models=sorted(self.selected), encoding=self.chosen_encoding()))

    def finish_selected_preview(self, kind, payload):
        if self.preview_job_revision == self.preview_revision:
            if kind == 'result':
                for key in ('preview', 'preview_simplified', 'preview_omitted_models', 'preview_note'):
                    self.scan[key] = payload[key]
                self.scan['preview_selected_models'] = sorted(self.selected)
                self.fit()
                self.status.set('Ready · preview allowance assigned to selected models')
            elif kind == 'cancelled':
                self.status.set('Preview update cancelled')
            else:
                self.status.set('Failed to refresh preview · ' + str(payload))
        if self.preview_pending and not self._preview_after:
            self._preview_after = self.root.after(0, self.refresh_selected_preview)
        self.update_controls()

    def export(self):
        if self.busy or self.preview_pending or not self.scan:
            return
        output = Path(self.output.get())
        overwrite = False
        if output.exists() or Path(str(output) + ".conversion.json").exists():
            overwrite = messagebox.askyesno("Replace existing output?", "Replace the existing DXF and its conversion report?", parent=self.root)
            if not overwrite:
                return
        options = Options(units=self.units.get(), models=sorted(self.selected), overwrite=overwrite, encoding=self.chosen_encoding())
        self.start_job("convert", options)

    def start_job(self, action, options):
        self.job_id += 1
        self.action = action
        self.busy = True
        self.messages = self.context.Queue()
        self.cancel_event = self.context.Event()
        self.process = self.context.Process(target=run_job, args=(self.messages, self.cancel_event, self.job_id, action, self.path, self.output.get(), asdict(options)), daemon=True)
        self.process.start()
        self.progress.start(12)
        self.status.set("Loading archive…" if action == "inspect" else "Exporting selected geometry…")
        self.update_controls()

    def cancel(self):
        if self.busy and self.cancel_event:
            self.preview_pending = False
            if self._preview_after:
                self.root.after_cancel(self._preview_after)
                self._preview_after = None
            self.cancel_event.set()
            self.status.set("Cancellation requested · waiting for safe cleanup…")
            self.cancel_button.configure(state="disabled")

    def poll(self):
        if self.messages is not None:
            try:
                while True:
                    job, kind, payload = self.messages.get_nowait()
                    if job != self.job_id:
                        continue
                    if kind == "progress":
                        if not self.cancel_event.is_set():
                            self.status.set(f"{payload[0]} · {payload[1]:,}")
                    else:
                        self.finish(kind, payload)
                        break
            except queue.Empty:
                if self.busy and self.process and not self.process.is_alive():
                    self.finish("error", "Conversion worker stopped unexpectedly.")
        if self.closing and not self.busy:
            self.destroy()
            return
        self.root.after(100, self.poll)

    def finish(self, kind, payload):
        self.busy = False
        self.progress.stop()
        self.process.join(timeout=.1)
        self.messages.close()
        self.messages = self.process = None
        if self.action == 'preview':
            self.finish_selected_preview(kind, payload)
            return
        if kind == "result":
            if self.action == "inspect":
                self.scan = payload
                self.selected = {m["name"] for m in payload["models"] if m["convertible"]}
                self.scan['preview_selected_models'] = sorted(self.selected)
                self.populate_models()
                self.fit()
            else:
                self.result = payload
                self.folder_button.configure(state="normal")
            counts = payload["source_counts"]
            warnings = payload["diagnostics"]["total"]
            self.status.set(f"{'DXF saved' if self.action == 'convert' else 'Ready'} · {counts['selected'] - counts['skipped']:,} convertible source objects · {counts['skipped']:,} skipped · {warnings:,} warnings")
        elif kind == "cancelled":
            self.status.set("Cancelled · existing files preserved")
        else:
            self.status.set("Failed · " + str(payload))
            if not self.closing:
                messagebox.showerror("Conversion failed", str(payload), parent=self.root)
        self.update_controls()

    def selected_preview(self):
        return [p for p in self.scan["preview"] if p["model"] in self.selected] if self.scan else []

    def fit(self):
        points = [p for item in self.selected_preview() for p in item["points"]]
        if not points:
            self.draw()
            return
        minx, miny = map(min, zip(*points))
        maxx, maxy = map(max, zip(*points))
        self.origin = ((minx + maxx) / 2, (miny + maxy) / 2)
        width, height = max(100, self.canvas.winfo_width()), max(100, self.canvas.winfo_height())
        self.scale = min((width - 70) / max(maxx - minx, 1), (height - 70) / max(maxy - miny, 1))
        self.offset = [width / 2, height / 2]
        self.fit_scale = self.scale
        self.zoom_percent.set("100%")
        self.draw()

    def zoom(self, factor, x=None, y=None):
        x = self.canvas.winfo_width() / 2 if x is None else x
        y = self.canvas.winfo_height() / 2 if y is None else y
        if not 1e-9 < self.scale * factor < 1e10:
            return
        self.offset = [x + (self.offset[0] - x) * factor, y + (self.offset[1] - y) * factor]
        self.scale *= factor
        self.zoom_percent.set(f"{self.scale / self.fit_scale * 100:.0f}%")
        self.draw()

    def pan(self, event):
        if self.drag:
            self.offset[0] += event.x - self.drag[0]
            self.offset[1] += event.y - self.drag[1]
            self.drag = (event.x, event.y)
            self.draw()

    def mouse_coordinates(self, event):
        if self.scan:
            x = self.origin[0] + (event.x - self.offset[0]) / self.scale
            y = self.origin[1] - (event.y - self.offset[1]) / self.scale
            self.cursor.set(f"X {x:,.3f}    Y {y:,.3f}    Plan view · Z = 0")

    def schedule_draw(self, event=None):
        if self._redraw_job:
            self.root.after_cancel(self._redraw_job)
        self._redraw_job = self.root.after(120, self.draw)

    def draw(self):
        self._redraw_job = None
        c = self.canvas
        c.delete("all")
        width, height = c.winfo_width(), c.winfo_height()
        for x in range(0, width, 100):
            c.create_line(x, 0, x, height, fill="#20262e")
        for y in range(0, height, 100):
            c.create_line(0, y, width, y, fill="#20262e")
        for item in self.selected_preview():
            points = [(self.offset[0] + (x - self.origin[0]) * self.scale, self.offset[1] - (y - self.origin[1]) * self.scale) for x, y in item["points"]]
            colour = PALETTE.get(item["colour"].lower(), "#d3e2ef")
            if not points:
                continue
            if item["kind"] == "text":
                c.create_text(*points[0], text=item["text"], fill=colour, anchor="sw", font=("Segoe UI", 9))
            elif item["kind"] == "point":
                x, y = points[0]
                c.create_line(x - 3, y, x + 3, y, fill=colour)
                c.create_line(x, y - 3, x, y + 3, fill=colour)
            elif len(points) > 1:
                c.create_line(*(v for point in points for v in point), fill=colour, width=1.2)
        if not self.scan:
            c.create_text(width / 2, height / 2, text="Open a 12d archive to preview plan geometry", fill="#9baec7", font=("Segoe UI", 12))
        c.create_polygon(30, 23, 19, 54, 30, 47, 41, 54, fill='#f0f5fb', outline='')
        c.create_text(30, 69, text='+Y', fill='#e5edf6', font=('Segoe UI', 9, 'bold'))
        if self.scan:
            label = "Plan preview · exact geometry in DXF"
            if self.scan["preview_simplified"]:
                label = "Simplified preview · export unaffected"
            if self.scan.get("preview_omitted_models"):
                label = "Too many models for preview allowance · some models not displayed"
            c.create_text(12, height - 14, text=label, fill="#b3c8df", anchor="w", font=("Segoe UI", 9))
        if self.units.get() in {"metres", "millimetres", "feet", "inches"}:
            available = min(160, max(60, width / 3))
            raw = available / self.scale
            power = 10 ** math.floor(math.log10(raw))
            length = min((n * power for n in (1, 2, 5, 10)), key=lambda n: abs(n * self.scale - available))
            pixels = length * self.scale
            left, bottom = width - 35 - pixels, height - 38
            suffix = {'metres': 'm', 'millimetres': 'mm', 'feet': 'ft', 'inches': 'in'}[self.units.get()]
            c.create_line(left, bottom, left + pixels, bottom, fill='#e7edf6', width=2)
            divisions = 4 if pixels >= 140 else 2
            for i in range(divisions + 1):
                x = left + pixels * i / divisions
                c.create_line(x, bottom, x, bottom - 7, fill='#e7edf6', width=1.5)
                text = f'{length * i / divisions:g}' + (f' {suffix}' if i == divisions else '')
                c.create_text(x, bottom - 18, text=text, fill='#e7edf6', font=('Segoe UI', 9))

    def text_dialog(self, title, text, *, show_support=False):
        from .ui_layout import FIELD, BORDER, INK, dark_titlebar
        dialog = tk.Toplevel(self.root)
        dialog.title(title)
        dialog.geometry("780x580")
        dialog.minsize(520, 350)
        dialog.transient(self.root)
        dialog.bind('<Map>', lambda e: dark_titlebar(dialog) if e.widget == dialog else None, add='+')
        heading = ttk.Frame(dialog, padding=(20, 16, 20, 10))
        heading.pack(fill='x')
        ttk.Label(heading, text=title, font=('Segoe UI', 14, 'bold')).pack(anchor='w')
        frame = ttk.Frame(dialog, padding=16)
        frame.pack(fill="both", expand=True)
        area = tk.Text(frame, wrap="word", padx=16, pady=14, font=("Segoe UI", 10),
                       relief='flat', highlightthickness=1, highlightbackground=BORDER,
                       background=FIELD, foreground=INK, selectbackground='#294f7c')
        scroll = ttk.Scrollbar(frame, command=area.yview)
        area.configure(yscrollcommand=scroll.set)
        scroll.pack(side="right", fill="y")
        area.pack(fill="both", expand=True)
        area.insert("1.0", text)
        area.configure(state="disabled")
        footer = ttk.Frame(dialog, padding=(16, 0, 16, 12))
        footer.pack(fill='x')
        if show_support:
            from . import support
            dialog.support_button = ttk.Button(footer, text='Buy us a coffee (PayPal)',
                command=lambda: support.open_support(dialog),
                state='normal' if support.support_url() else 'disabled')
            dialog.support_button.pack(side='left')
        ttk.Button(footer, text="Close", command=dialog.destroy).pack(side='right')
        dialog.bind("<Escape>", lambda e: dialog.destroy())
        area.focus_set()
        return dialog

    def show_guide(self):
        return self.text_dialog("User Guide", resource_text("user_guide.txt"))

    def show_about(self):
        from . import support
        link = support.support_url()
        support_text = f'\n\n{support.COFFEE_MESSAGE}\nSupport is optional.'
        if link:
            support_text += f'\n{link}'
        return self.text_dialog("About 12dAZ Converter", f"12dAZ Converter\nVersion {__version__} · build {__version__}\n\n12daz / 12da to 2D DXF\nOffline desktop converter\n\n{COPYRIGHT}\n\nLicensed under the MIT License.\n\nSee Help → License and Third-party Notices for the full license and bundled dependency notices." + support_text, show_support=True)

    def show_licenses(self):
        return self.text_dialog("License and Third-party Notices", resource_text("LICENSE.txt") + "\n\n" + resource_text("THIRD_PARTY_NOTICES.txt"))

    def show_report(self):
        report = self.result or self.scan
        if report:
            self.text_dialog("Conversion report", json.dumps({k: v for k, v in report.items() if k != "preview"}, indent=2, ensure_ascii=False))

    def open_folder(self):
        if self.result and self.result.get("published"):
            path = str(Path(self.result["output"]).parent)
            if sys.platform == "win32":
                os.startfile(path)
            elif sys.platform == "darwin":
                subprocess.Popen(["open", path])
            else:
                subprocess.Popen(["xdg-open", path])

    def close(self):
        if self.busy:
            if messagebox.askyesno("Exit after cancellation?", "Cancel the current job and close after cleanup?", parent=self.root):
                self.closing = True
                self.cancel()
        else:
            self.destroy()

    def destroy(self):
        for timer in self.root.tk.call('after', 'info'):
            self.root.after_cancel(timer)
        self.root.destroy()


def main():
    mp.freeze_support()
    from .splash import launch_gui
    root = tk.Tk()
    splash = launch_gui(root, ConverterApp)
    root.mainloop()
    if splash.startup_error:
        raise splash.startup_error


if __name__ == "__main__":
    main()
