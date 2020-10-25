use std::marker::PhantomData;
use imgui::sys as sys;
use super::proxy::Key;

pub struct FocusOrder<'a> {
    windows: &'a [*mut sys::ImGuiWindow],
    _phantom: PhantomData<&'a mut ()>,
}

pub fn focus_order(_context: &mut imgui::Context) -> FocusOrder<'_> {
    unsafe {
        let context =
            sys::igGetCurrentContext().as_mut().expect("Current imgui context");
        let windows = context.WindowsFocusOrder;

        let windows: &[*mut sys::ImGuiWindow] = std::slice::from_raw_parts(windows.Data, windows.Size as _);

        FocusOrder{windows, _phantom: PhantomData}
    }
}

impl<'a> Iterator for FocusOrder<'a> {
    type Item = (&'a imgui::ImStr, Key);

    fn next(&mut self) -> Option<Self::Item> {
        while let Some((ret, rest)) = self.windows.split_first() {
            self.windows = rest;
            match unsafe{ret.as_ref()} {
                Some(window) if window.ViewportOwned => {
                    let name = unsafe{imgui::ImStr::from_ptr_unchecked(window.Name)};
                    return Some((name, window.ViewportId))
                },
                _ => continue,
            }
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(self.windows.len()))
    }
}
