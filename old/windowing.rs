use core::ops::{Deref, DerefMut};
use imgui::sys as imgui_sys;
use imgui_sys::{ImGuiPlatformIO, ImGuiPlatformMonitor, ImGuiViewport, ImVec2};
use std::{cell::RefCell, collections::HashMap, rc::Rc};
use winit::{
    event_loop::EventLoopWindowTarget,
    window::{Window, WindowId, WindowBuilder},
};

#[derive(Debug)]
pub struct Outlet {
    surface: wgpu::Surface,
    sc_desc: wgpu::SwapChainDescriptor,
    swap_chain: Option<wgpu::SwapChain>,
}
impl Outlet {
    pub fn format() -> wgpu::TextureFormat {
        wgpu::TextureFormat::Bgra8Unorm
    }
    fn desc() -> wgpu::SwapChainDescriptor {
        wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            format: Self::format(),
            width: 0,
            height: 0,
            present_mode: wgpu::PresentMode::Fifo,
        }
    }
}

#[derive(Debug)]
pub struct NativeWindow {
    pub native: Window,
    pub outlet: Outlet,
    focus: bool,
    pub minimized: bool,
}
impl NativeWindow {
    fn from_native(native: Window, instance: &wgpu::Instance) -> Self {
        let surface = unsafe { instance.create_surface(&native) };
        Self::with_surface(native, surface)
    }
    fn with_surface(native: Window, surface: wgpu::Surface) -> Self {
        let outlet = Outlet {
            surface,
            sc_desc: Outlet::desc(),
            swap_chain: None,
        };
        NativeWindow {
            native,
            focus: true,
            minimized: false,
            outlet,
        }
    }
    pub fn get_current_frame(
        &mut self,
        device: &wgpu::Device,
    ) -> Result<Option<wgpu::SwapChainFrame>, wgpu::SwapChainError> {
        if self.minimized {
            return Ok(None);
        }
        if self.outlet.swap_chain.is_none() {
            self.create_swap_chain(device);
        }
        self.outlet
            .swap_chain
            .as_mut()
            .unwrap()
            .get_current_frame()
            .map(|ok| Some(ok))
    }
    fn create_swap_chain(&mut self, device: &wgpu::Device) {
        let outlet = &mut self.outlet;
        let size = self.native.inner_size();
        outlet.sc_desc.width = size.width;
        outlet.sc_desc.height = size.height;
        outlet.swap_chain = Some(device.create_swap_chain(&outlet.surface, &outlet.sc_desc));
    }
}
pub struct Manager {
    windows: HashMap<WindowId, NativeWindow>,
    instance: wgpu::Instance,
}
impl Manager {
    pub fn from_parts(instance: wgpu::Instance, native: Window, surface: wgpu::Surface) -> Self {
        let mut windows = HashMap::new();
        let window = NativeWindow::with_surface(native, surface);
        windows.insert(window.native.id(), window);
        Self { windows, instance }
    }
    pub fn set_focus(&mut self, wid: WindowId, focus: bool) {
        self.windows.get_mut(&wid).unwrap().focus = focus;
    }
    pub fn with_loop<'a, T>(
        &'a mut self,
        event_loop: &'a EventLoopWindowTarget<T>,
    ) -> ActiveManager<'a, T> {
        ActiveManager {
            manager: self,
            event_loop,
        }
    }
    #[track_caller]
    pub fn expect_native_window(&self, wid: WindowId) -> &Window {
        self.get_native_window(wid).expect("Expect native window")
    }
    pub fn get_native_window(&self, wid: WindowId) -> Option<&Window> {
        self.windows.get(&wid).map(|window| &window.native)
    }
    #[track_caller]
    pub fn expect_window_mut(&mut self, wid: WindowId) -> &mut NativeWindow {
        self.windows.get_mut(&wid).expect("Expect window")
    }
    pub fn make_window_dirty(&mut self, wid: WindowId) {
        self.windows.get_mut(&wid).unwrap().outlet.swap_chain = None;
    }
    pub fn maintain_outlets(&mut self, device: &wgpu::Device) {
        for window in self.windows.values_mut() {
            if window.outlet.swap_chain.is_none() {
                window.create_swap_chain(device);
            }
        }
    }
    pub fn close_native_window(&mut self, wid: WindowId) -> bool {
        self.windows.remove(&wid);
        self.windows.is_empty()
    }
}

pub struct ActiveManager<'a, T: 'static> {
    pub manager: &'a mut Manager,
    event_loop: &'a EventLoopWindowTarget<T>,
}
impl<'a, T> ActiveManager<'a, T> {
    pub fn spawn_native_window(&mut self, decorations: bool) -> WindowId {
        let native = WindowBuilder::new().with_decorations(decorations).build(self.event_loop).unwrap();
        let wid = native.id();
        let window = NativeWindow::from_native(native, &self.manager.instance);
        self.manager.windows.insert(wid, window);
        wid
    }
}

impl<'a, T: 'static> Deref for ActiveManager<'a, T> {
    type Target = Manager;
    fn deref(&self) -> &Self::Target {
        &self.manager
    }
}
impl<'a, T: 'static> DerefMut for ActiveManager<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.manager
    }
}

struct CacheData {
    size: ImVec2,
    pos: ImVec2,
    focus: bool,
    minimized: bool,
}
struct Cache {
    wid: winit::window::WindowId,
    data: Option<CacheData>,
}
#[derive(Debug)]
struct Command {
    key: Key,
    kind: Kind,
}
#[derive(Debug)]
enum Kind {
    CreateWindow{decorations: bool},
    DestroyWindow,
    ShowWindow,
    SetPos(ImVec2),
    SetSize(ImVec2),
    SetFocus,
    SetTitle(String),
}
pub struct Proxy {
    windows: HashMap<Key, Cache>,
    commands: Vec<Command>,
    next_id: Key,
}
pub type SharedProxy = Rc<RefCell<Proxy>>;
impl Proxy {
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
            commands: vec![],
            next_id: 1,
        }
    }
    pub fn shared() -> SharedProxy {
        Rc::new(RefCell::new(Self::new()))
    }
    pub fn use_window(&mut self, wid: WindowId) -> Key {
        let cache = Cache { wid, data: None };
        let key = self.next_key();
        self.windows.insert(key, cache);
        key
    }
    pub fn update<'a, T>(&mut self, manager: &mut ActiveManager<'a, T>) {
        if !self.commands.is_empty() {
            dbg!(&self.commands);
        }
        for Command { key, kind } in self.commands.drain(..) {
            match &kind {
                Kind::CreateWindow{decorations} => {
                    let wid = manager.spawn_native_window(*decorations);
                    let cache = Cache { wid, data: None };
                    self.windows.insert(key, cache);
                }
                Kind::DestroyWindow => {
                    let wid = self.windows.remove(&key).unwrap().wid;
                    manager.manager.windows.remove(&wid);
                }
                _ => {
                    let wid = self.windows.get(&key).unwrap().wid;
                    let window = manager.manager.windows.get_mut(&wid).unwrap();
                    match kind {
                        Kind::CreateWindow{..} | Kind::DestroyWindow => unreachable!(),
                        Kind::ShowWindow => {
                            window.native.set_visible(true);
                        }
                        Kind::SetPos(pos) => {
                            let pos = winit::dpi::PhysicalPosition {
                                x: pos.x.round() as i32,
                                y: pos.y.round() as i32,
                            };
                            window.native.set_outer_position(pos);
                        }
                        Kind::SetSize(size) => {
                            let size = winit::dpi::PhysicalSize {
                                width: size.x.round() as u32,
                                height: size.y.round() as u32,
                            };
                            window.native.set_inner_size(size);
                            window.outlet.swap_chain = None;
                        }
                        Kind::SetFocus => {
                            //unimplemented!();
                        }
                        Kind::SetTitle(title) => window.native.set_title(&title),
                    }
                }
            }
        }
        for (_key, cache) in &mut self.windows {
            let wid = cache.wid;
            let window = manager.manager.windows.get(&wid).unwrap();
            let size = window.native.inner_size();
            let pos = window.native.outer_position().unwrap();
            let data = CacheData {
                size: ImVec2 {
                    x: size.width as _,
                    y: size.height as _,
                },
                pos: ImVec2 {
                    x: pos.x as _,
                    y: pos.y as _,
                },
                focus: window.focus,
                minimized: window.minimized,
            };
            cache.data = Some(data);
        }
    }
    fn next_key(&mut self) -> Key {
        let key = self.next_id;
        self.next_id += 1;
        key
    }
    pub fn create_window(&mut self, decorations: bool) -> Key {
        let key = self.next_key();
        self.commands.push(Command {
            key,
            kind: Kind::CreateWindow{decorations},
        });
        key
    }
    pub fn destroy_window(&mut self, key: Key) {
        self.commands.push(Command {
            key,
            kind: Kind::DestroyWindow,
        });
    }
    pub fn show_window(&mut self, key: Key) {
        self.commands.push(Command {
            key,
            kind: Kind::ShowWindow,
        });
    }
    pub fn set_position(&mut self, key: Key, pos: ImVec2) {
        self.make_dirty(key);
        self.commands.push(Command {
            key,
            kind: Kind::SetPos(pos),
        });
    }
    pub fn set_size(&mut self, key: Key, size: ImVec2) {
        self.make_dirty(key);
        self.commands.push(Command {
            key,
            kind: Kind::SetSize(size),
        });
    }
    pub fn set_focus(&mut self, key: Key) {
        self.make_dirty(key);
        self.commands.push(Command {
            key,
            kind: Kind::SetFocus,
        });
    }
    pub fn get_position(&self, key: Key) -> ImVec2 {
        self.expect_data_from_key(key).pos
    }
    pub fn get_size(&self, key: Key) -> ImVec2 {
        self.expect_data_from_key(key).size
    }
    pub fn get_focus(&self, key: Key) -> bool {
        self.expect_data_from_key(key).focus
    }
    pub fn get_minimized(&self, key: Key) -> bool {
        self.expect_data_from_key(key).minimized
    }
    pub fn set_title(&mut self, key: Key, title: String) {
        self.commands.push(Command {
            key,
            kind: Kind::SetTitle(title),
        });
    }
    pub fn draw_data<F>(
        &self,
        manager: &mut Manager,
        imgui: &mut imgui::Context,
        mut callback: F,
    ) where
        F: FnMut(&mut NativeWindow, &imgui::DrawData),
    {
        use imgui::internal::RawCast;
        let platform = imgui.platform_mut();
        let windows = &mut manager.windows;
        unsafe {
            let viewports =
                std::slice::from_raw_parts(platform.Viewports.Data, platform.Viewports.Size as _);
            for vp in viewports.iter() {
                if vp.is_null() {
                    continue;
                }
                let vp = &(**vp);
                if vp.DrawData.is_null() || vp.PlatformUserData.is_null() {
                    continue;
                }
                let key: Key = std::mem::transmute(vp.PlatformUserData);
                let cache = self.windows.get(&key).unwrap();
                let window = windows.get_mut(&cache.wid).unwrap();
                let draw_data = RawCast::from_raw(&*vp.DrawData);
                callback(window, draw_data);
            }
        }
    }
    fn make_dirty(&mut self, key: Key) {
        /*if let Some(window) = self.window.get_mut(key) {
            window.dirty = true;
        }*/
        if let Some(window) = self.windows.get_mut(&key) {
            window.data = None;
        }
    }
    fn expect_data_from_key(&self, key: Key) -> &CacheData {
        let window = self.windows.get(&key).unwrap();
        window.data.as_ref().unwrap()
    }
}

pub unsafe fn from_vp<R: 'static, F: FnOnce(&mut Proxy, &mut Key) -> R>(
    vp: *mut ImGuiViewport,
    callback: F,
) -> R {
    let vp = &mut (*vp);
    let ptr = (*imgui_sys::igGetIO()).BackendPlatformUserData;
    assert_eq!(ptr.is_null(), false);
    let proxy: SharedProxy = Rc::from_raw(ptr as _);
    let ret = {
        let mut guard = proxy.borrow_mut();
        let key: &mut Key = std::mem::transmute(&mut vp.PlatformUserData);
        callback(&mut *guard, key)
    };
    std::mem::forget(proxy);
    ret
}

pub fn register_platform(imgui: &mut imgui::Context, window: &Window) -> SharedProxy {
    update_monitors(&window, imgui, true);
    let platform = imgui.platform_mut();
    //update_monitors(&window, platform, true);

    unsafe extern "C" fn create_window(vp: *mut ImGuiViewport) {
        from_vp(vp, |proxy, key| {
            assert_eq!(*key, 0);
            *key = proxy.create_window((*vp).Flags as u32 & imgui_sys::ImGuiViewportFlags_NoDecoration == 0);
            //dbg!(key);
            //dbg!((*vp).PlatformUserData);
        });
    }
    platform.Platform_CreateWindow = Some(create_window);

    unsafe extern "C" fn destroy_window(vp: *mut ImGuiViewport) {
        from_vp(vp, |proxy, key| {
            proxy.destroy_window(*key);
            *key = 0;
        });
    }
    platform.Platform_DestroyWindow = Some(destroy_window);

    unsafe extern "C" fn show_window(vp: *mut ImGuiViewport) {
        from_vp(vp, |proxy, key| {
            proxy.show_window(*key);
        });
    }
    platform.Platform_ShowWindow = Some(show_window);

    unsafe extern "C" fn set_window_pos(vp: *mut ImGuiViewport, pos: ImVec2) {
        from_vp(vp, |proxy, key| {
            proxy.set_position(*key, pos);
        });
    }
    platform.Platform_SetWindowPos = Some(set_window_pos);

    unsafe extern "C" fn get_window_pos(vp: *mut ImGuiViewport, pos: *mut ImVec2) {
        /*if (*vp).PlatformUserData as usize > 1 {
            println!("get_window_pos!!!");
        }*/
        *pos = from_vp(vp, |proxy, key| proxy.get_position(*key));
    }
    unsafe{ImGuiPlatformIO_Set_Platform_GetWindowPos(platform, get_window_pos);}

    unsafe extern "C" fn set_window_size(vp: *mut ImGuiViewport, size: ImVec2) {
        from_vp(vp, |proxy, key| {
            proxy.set_size(*key, size);
        })
    }
    platform.Platform_SetWindowSize = Some(set_window_size);

    unsafe extern "C" fn get_window_size(vp: *mut ImGuiViewport, size: *mut ImVec2) {
        *size = from_vp(vp, |proxy, key| proxy.get_size(*key));
    }
    unsafe{ImGuiPlatformIO_Set_Platform_GetWindowSize(platform, get_window_size);}

    unsafe extern "C" fn set_window_focus(vp: *mut ImGuiViewport) {
        from_vp(vp, |proxy, key| {
            proxy.set_focus(*key);
        });
    }
    platform.Platform_SetWindowFocus = Some(set_window_focus);

    unsafe extern "C" fn get_window_focus(vp: *mut ImGuiViewport) -> bool {
        from_vp(vp, |proxy, key| proxy.get_focus(*key))
    }
    platform.Platform_GetWindowFocus = Some(get_window_focus);

    unsafe extern "C" fn get_window_minimized(vp: *mut ImGuiViewport) -> bool {
        from_vp(vp, |proxy, key| proxy.get_minimized(*key))
    }
    platform.Platform_GetWindowMinimized = Some(get_window_minimized);

    unsafe extern "C" fn set_window_title(
        vp: *mut ImGuiViewport,
        str: *const ::std::os::raw::c_char,
    ) {
        let title = std::ffi::CStr::from_ptr(str).to_bytes();
        from_vp(vp, |proxy, key| {
            proxy.set_title(*key, std::str::from_utf8(title).unwrap().to_owned());
        });
    }
    platform.Platform_SetWindowTitle = Some(set_window_title);

    dbg!(&platform);

    let proxy = Proxy::shared();
    let key = proxy.borrow_mut().use_window(window.id());

    unsafe {
        (*platform.MainViewport).PlatformUserData = key as _;

        use imgui::internal::RawCast;
        imgui.io_mut().raw_mut().BackendPlatformUserData = Rc::into_raw(Rc::clone(&proxy)) as _;
    }

    proxy
}

pub fn update_monitors(window: &Window, /*platform: &mut ImGuiPlatformIO,*/ imgui: &mut imgui::Context, first_time: bool) {
    /*unsafe {
        let viewports = std::slice::from_raw_parts(platform.Viewports.Data, platform.Viewports.Size as _);
        for viewport in viewports.iter().skip(1) {
            dbg!(**viewport);
        }
    }*/
    let platform = imgui.platform_mut();
    let mut monitors = if platform.Monitors.Data.is_null() {
        Vec::with_capacity(window.available_monitors().size_hint().0)
    } else {
        assert_eq!(first_time, false);
        use std::mem::replace;
        let raw = &mut platform.Monitors;
        let ptr = replace(&mut raw.Data, std::ptr::null_mut());
        let length = replace(&mut raw.Size, 0) as usize;
        let capacity = replace(&mut raw.Capacity, 0) as usize;
        assert!(length < 32);
        assert!(capacity <= length);
        unsafe { Vec::from_raw_parts(ptr, length, capacity) }
    };
    monitors.clear();
    monitors.extend(window.available_monitors().take(32).map(|monitor| {
        let pos = monitor.position();
        let posf = ImVec2 {
            x: pos.x as _,
            y: pos.y as _,
        };
        let size = monitor.size();
        let sizef = ImVec2 {
            x: size.width as _,
            y: size.height as _,
        };

        ImGuiPlatformMonitor {
            MainPos: posf,
            MainSize: sizef,
            WorkPos: posf,
            WorkSize: sizef,
            DpiScale: monitor.scale_factor() as _,
        }
    }));
    //let (ptr, length, capacity) = monitors.into_raw_parts();
    use std::convert::TryInto;
    let (ptr, length, capacity) = (monitors.as_mut_ptr(), monitors.len(), monitors.capacity());
    std::mem::forget(monitors);
    let raw = &mut platform.Monitors;
    raw.Capacity = capacity as _;
    raw.Size = length as _;
    raw.Data = ptr;
}

type Platform_Get_Callback = unsafe extern "C" fn(*mut ImGuiViewport, *mut ImVec2);
extern "C" {
    //void ImGuiPlatformIO_Set_Platform_GetWindowPos(ImGuiPlatformIO* platform_io, void(*user_callback)(ImGuiViewport* vp, ImVec2* out_pos))
    fn ImGuiPlatformIO_Set_Platform_GetWindowPos(platform_io: &mut ImGuiPlatformIO, user_ballback: Platform_Get_Callback);
    //void ImGuiPlatformIO_Set_Platform_GetWindowSize(ImGuiPlatformIO* platform_io, void(*user_callback)(ImGuiViewport* vp, ImVec2* out_size))
    fn ImGuiPlatformIO_Set_Platform_GetWindowSize(platform_io: &mut ImGuiPlatformIO, user_ballback: Platform_Get_Callback);
}

trait GetPlatformIO {
    fn platform_mut(&mut self) -> &mut imgui_sys::ImGuiPlatformIO;
}
impl GetPlatformIO for imgui::Context {
    fn platform_mut(&mut self) -> &mut imgui_sys::ImGuiPlatformIO {
        unsafe {
            &mut *(imgui_sys::igGetPlatformIO())
        }
    }
}
