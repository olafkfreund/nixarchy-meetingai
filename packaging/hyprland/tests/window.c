// Deterministic Wayland fixture: solid RGB window and optional contrasting
// popup.
#define _GNU_SOURCE
#include "xdg-shell-client-protocol.h"
#include <assert.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
#include <wayland-client.h>

static struct wl_compositor *compositor;
static struct wl_shm *shm;
static struct xdg_wm_base *shell;
static struct wl_surface *surface;
static struct xdg_surface *xdg;
static uint32_t color;
static int width = 320, height = 200, want_popup;
static volatile sig_atomic_t running = 1;

static struct wl_buffer *buffer(int w, int h, uint32_t rgb) {
  int fd = memfd_create("lennette-test", MFD_CLOEXEC);
  assert(fd >= 0 && ftruncate(fd, (off_t)w * h * 4) == 0);
  uint32_t *pixels =
      mmap(NULL, (size_t)w * h * 4, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
  assert(pixels != MAP_FAILED);
  for (int i = 0; i < w * h; i++)
    pixels[i] = 0xff000000 | rgb;
  struct wl_shm_pool *pool = wl_shm_create_pool(shm, fd, w * h * 4);
  struct wl_buffer *result =
      wl_shm_pool_create_buffer(pool, 0, w, h, w * 4, WL_SHM_FORMAT_ARGB8888);
  wl_shm_pool_destroy(pool);
  munmap(pixels, (size_t)w * h * 4);
  close(fd);
  return result;
}
static void paint(struct wl_surface *s, int w, int h, uint32_t rgb) {
  wl_surface_attach(s, buffer(w, h, rgb), 0, 0);
  wl_surface_damage(s, 0, 0, w, h);
  wl_surface_commit(s);
}
static void popup_configure(void *data, struct xdg_surface *s,
                            uint32_t serial) {
  xdg_surface_ack_configure(s, serial);
  paint(data, 100, 60, 0x00ffff);
}
static const struct xdg_surface_listener popup_surface_listener = {
    .configure = popup_configure};
static void popup_geometry(void *d, struct xdg_popup *p, int32_t x, int32_t y,
                           int32_t w, int32_t h) {
  (void)d;
  (void)p;
  (void)x;
  (void)y;
  (void)w;
  (void)h;
}
static void popup_done(void *d, struct xdg_popup *p) {
  (void)d;
  (void)p;
}
static const struct xdg_popup_listener popup_listener = {
    .configure = popup_geometry, .popup_done = popup_done};
static void make_popup(void) {
  struct wl_surface *s = wl_compositor_create_surface(compositor);
  struct xdg_surface *xs = xdg_wm_base_get_xdg_surface(shell, s);
  struct xdg_positioner *positioner = xdg_wm_base_create_positioner(shell);
  xdg_positioner_set_size(positioner, 100, 60);
  xdg_positioner_set_anchor_rect(positioner, width - 10, 30, 10, 10);
  xdg_positioner_set_anchor(positioner, XDG_POSITIONER_ANCHOR_RIGHT);
  xdg_positioner_set_gravity(positioner, XDG_POSITIONER_GRAVITY_RIGHT);
  struct xdg_popup *popup = xdg_surface_get_popup(xs, xdg, positioner);
  xdg_popup_add_listener(popup, &popup_listener, NULL);
  xdg_positioner_destroy(positioner);
  xdg_surface_add_listener(xs, &popup_surface_listener, s);
  wl_surface_commit(s);
}
static void configure(void *d, struct xdg_surface *s, uint32_t serial) {
  (void)d;
  xdg_surface_ack_configure(s, serial);
  paint(surface, width, height, color);
  static int opened;
  if (want_popup && !opened) {
    opened = 1;
    make_popup();
  }
}
static const struct xdg_surface_listener surface_listener = {.configure =
                                                                 configure};
static void size_changed(void *d, struct xdg_toplevel *t, int32_t w, int32_t h,
                         struct wl_array *states) {
  (void)d;
  (void)t;
  (void)states;
  if (w > 0)
    width = w;
  if (h > 0)
    height = h;
}
static void closed(void *d, struct xdg_toplevel *t) {
  (void)d;
  (void)t;
  running = 0;
}
static const struct xdg_toplevel_listener toplevel_listener = {
    .configure = size_changed, .close = closed};
static void ping(void *d, struct xdg_wm_base *s, uint32_t serial) {
  (void)d;
  xdg_wm_base_pong(s, serial);
}
static const struct xdg_wm_base_listener shell_listener = {.ping = ping};
static void global(void *d, struct wl_registry *r, uint32_t id,
                   const char *iface, uint32_t version) {
  (void)d;
  (void)version;
  if (!strcmp(iface, "wl_compositor"))
    compositor = wl_registry_bind(r, id, &wl_compositor_interface, 4);
  else if (!strcmp(iface, "wl_shm"))
    shm = wl_registry_bind(r, id, &wl_shm_interface, 1);
  else if (!strcmp(iface, "xdg_wm_base"))
    shell = wl_registry_bind(r, id, &xdg_wm_base_interface, 1);
}
static void removed(void *d, struct wl_registry *r, uint32_t id) {
  (void)d;
  (void)r;
  (void)id;
}
static const struct wl_registry_listener registry_listener = {
    .global = global, .global_remove = removed};
static void stop(int sig) {
  (void)sig;
  running = 0;
}
int main(int argc, char **argv) {
  if (argc < 3) {
    fprintf(stderr, "usage: %s APP_ID RGB_HEX [popup]\n", argv[0]);
    return 2;
  }
  color = strtoul(argv[2], NULL, 16);
  want_popup = argc > 3;
  struct wl_display *display = wl_display_connect(NULL);
  assert(display);
  struct wl_registry *registry = wl_display_get_registry(display);
  wl_registry_add_listener(registry, &registry_listener, NULL);
  assert(wl_display_roundtrip(display) >= 0 && compositor && shm && shell);
  xdg_wm_base_add_listener(shell, &shell_listener, NULL);
  surface = wl_compositor_create_surface(compositor);
  xdg = xdg_wm_base_get_xdg_surface(shell, surface);
  xdg_surface_add_listener(xdg, &surface_listener, NULL);
  struct xdg_toplevel *toplevel = xdg_surface_get_toplevel(xdg);
  xdg_toplevel_add_listener(toplevel, &toplevel_listener, NULL);
  xdg_toplevel_set_app_id(toplevel, argv[1]);
  xdg_toplevel_set_title(toplevel, argv[1]);
  wl_surface_commit(surface);
  signal(SIGTERM, stop);
  signal(SIGINT, stop);
  while (running && wl_display_dispatch(display) >= 0) {
  }
  wl_display_disconnect(display);
  return 0;
}
