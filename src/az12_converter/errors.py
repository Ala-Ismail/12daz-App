class ConversionError(Exception):
    """An input, option or publication error safe to show to the user."""


class Cancelled(ConversionError):
    pass


def checkpoint(cancel=None):
    if cancel is not None and cancel.is_set():
        raise Cancelled("Conversion cancelled before publication.")
