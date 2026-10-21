// Export pixels through real capture protocols, using either SHM or DMA-BUF.
#define _GNU_SOURCE
#include "ext-foreign-toplevel-list-v1-client-protocol.h"
#include "ext-image-capture-source-v1-client-protocol.h"
#include "ext-image-copy-capture-v1-client-protocol.h"
#include "linux-dmabuf-unstable-v1-client-protocol.h"
#include "wlr-screencopy-unstable-v1-client-protocol.h"
#include <assert.h>
#include <fcntl.h>
#include <gbm.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
#include <wayland-client.h>

static struct wl_display *display;
static struct wl_shm *shm;
static struct zwp_linux_dmabuf_v1 *dmabuf;
static struct zwlr_screencopy_manager_v1 *wlr;
static struct ext_image_copy_capture_manager_v1 *ext;
static struct ext_output_image_capture_source_manager_v1 *output_sources;
static struct ext_foreign_toplevel_image_capture_source_manager_v1
    *window_sources;
static struct ext_foreign_toplevel_list_v1 *toplevels;
static struct ext_foreign_toplevel_handle_v1 *target_window;
static const char *target_name;
static uint32_t width, height, stride, format;
static bool done, failed, constraints_done, use_dma, have_format;
static void *pixels;
static struct wl_buffer *buffer;
static struct gbm_device *device;
static struct gbm_bo *bo;
struct output {
  struct wl_output *resource;
  char name[128];
};
static struct output outputs[16];
static unsigned output_count;

static void geometry(void *d, struct wl_output *o, int32_t x, int32_t y,
                     int32_t pw, int32_t ph, int32_t sub, const char *m,
                     const char *n, int32_t t) {
  (void)d;
  (void)o;
  (void)x;
  (void)y;
  (void)pw;
  (void)ph;
  (void)sub;
  (void)m;
  (void)n;
  (void)t;
}
static void mode(void *d, struct wl_output *o, uint32_t f, int32_t w, int32_t h,
                 int32_t r) {
  (void)d;
  (void)o;
  (void)f;
  (void)w;
  (void)h;
  (void)r;
}
static void output_done(void *d, struct wl_output *o) {
  (void)d;
  (void)o;
}
static void scale(void *d, struct wl_output *o, int32_t s) {
  (void)d;
  (void)o;
  (void)s;
}
static void name(void *d, struct wl_output *o, const char *n) {
  (void)o;
  snprintf(((struct output *)d)->name, 128, "%s", n);
}
static void description(void *d, struct wl_output *o, const char *s) {
  (void)d;
  (void)o;
  (void)s;
}
static const struct wl_output_listener output_listener = {
    geometry, mode, output_done, scale, name, description};
static void handle_closed(void *d, struct ext_foreign_toplevel_handle_v1 *h) {
  (void)d;
  if (target_window == h)
    target_window = NULL;
}
static void handle_done(void *d, struct ext_foreign_toplevel_handle_v1 *h) {
  (void)d;
  (void)h;
}
static void handle_string(void *d, struct ext_foreign_toplevel_handle_v1 *h,
                          const char *s) {
  (void)d;
  (void)h;
  (void)s;
}
static void handle_app(void *d, struct ext_foreign_toplevel_handle_v1 *h,
                       const char *s) {
  (void)d;
  if (!strcmp(s, target_name))
    target_window = h;
}
static const struct ext_foreign_toplevel_handle_v1_listener handle_listener = {
    handle_closed, handle_done, handle_string, handle_app, handle_string};
static void toplevel(void *d, struct ext_foreign_toplevel_list_v1 *l,
                     struct ext_foreign_toplevel_handle_v1 *h) {
  (void)d;
  (void)l;
  ext_foreign_toplevel_handle_v1_add_listener(h, &handle_listener, NULL);
}
static void finished(void *d, struct ext_foreign_toplevel_list_v1 *l) {
  (void)d;
  (void)l;
}
static const struct ext_foreign_toplevel_list_v1_listener list_listener = {
    toplevel, finished};
static void global(void *d, struct wl_registry *r, uint32_t id,
                   const char *iface, uint32_t v) {
  (void)d;
#define BIND(IFACE, VAR, VERSION)                                              \
  if (!strcmp(iface, #IFACE)) {                                                \
    VAR = wl_registry_bind(r, id, &IFACE##_interface, VERSION);                \
    return;                                                                    \
  }
  BIND(wl_shm, shm, 1)
  BIND(zwp_linux_dmabuf_v1, dmabuf, v < 3 ? v : 3)
  BIND(zwlr_screencopy_manager_v1, wlr, v < 3 ? v : 3)
  BIND(ext_image_copy_capture_manager_v1, ext, 1)
  BIND(ext_output_image_capture_source_manager_v1, output_sources, 1)
  BIND(ext_foreign_toplevel_image_capture_source_manager_v1, window_sources, 1)
  if (!strcmp(iface, "ext_foreign_toplevel_list_v1")) {
    toplevels =
        wl_registry_bind(r, id, &ext_foreign_toplevel_list_v1_interface, 1);
    ext_foreign_toplevel_list_v1_add_listener(toplevels, &list_listener, NULL);
  } else if (!strcmp(iface, "wl_output")) {
    assert(output_count < 16 && v >= 4);
    struct output *out = &outputs[output_count++];
    out->resource = wl_registry_bind(r, id, &wl_output_interface, 4);
    wl_output_add_listener(out->resource, &output_listener, out);
  }
#undef BIND
}
static void removed(void *d, struct wl_registry *r, uint32_t id) {
  (void)d;
  (void)r;
  (void)id;
}
static const struct wl_registry_listener registry_listener = {global, removed};
static void allocate(void) {
  assert(width && height && have_format);
  if (use_dma) {
    const char *node = getenv("TEST_DRM_NODE");
    int fd = open(node ? node : "/dev/dri/renderD128", O_RDWR | O_CLOEXEC);
    assert(fd >= 0 && dmabuf);
    device = gbm_create_device(fd);
    assert(device);
    bo = gbm_bo_create(device, width, height, format,
                       GBM_BO_USE_RENDERING | GBM_BO_USE_LINEAR);
    assert(bo && gbm_bo_get_plane_count(bo) == 1);
    int buffer_fd = gbm_bo_get_fd(bo);
    uint64_t modifier = gbm_bo_get_modifier(bo);
    struct zwp_linux_buffer_params_v1 *params =
        zwp_linux_dmabuf_v1_create_params(dmabuf);
    zwp_linux_buffer_params_v1_add(params, buffer_fd, 0, 0,
                                   gbm_bo_get_stride(bo), modifier >> 32,
                                   modifier & 0xffffffff);
    buffer = zwp_linux_buffer_params_v1_create_immed(params, width, height,
                                                     format, 0);
    zwp_linux_buffer_params_v1_destroy(params);
    close(buffer_fd);
  } else {
    int fd = memfd_create("lennette-capture", MFD_CLOEXEC);
    if (!stride)
      stride = width * 4;
    assert(fd >= 0 && ftruncate(fd, (off_t)stride * height) == 0);
    pixels = mmap(NULL, (size_t)stride * height, PROT_READ | PROT_WRITE,
                  MAP_SHARED, fd, 0);
    assert(pixels != MAP_FAILED);
    struct wl_shm_pool *pool = wl_shm_create_pool(shm, fd, stride * height);
    buffer = wl_shm_pool_create_buffer(pool, 0, width, height, stride, format);
    wl_shm_pool_destroy(pool);
    close(fd);
  }
}
static void wlr_buffer(void *d, struct zwlr_screencopy_frame_v1 *f,
                       uint32_t fmt, uint32_t w, uint32_t h, uint32_t s) {
  (void)d;
  (void)f;
  if (!use_dma) {
    format = fmt;
    width = w;
    height = h;
    stride = s;
    have_format = true;
  }
}
static void wlr_flags(void *d, struct zwlr_screencopy_frame_v1 *f,
                      uint32_t flags) {
  (void)d;
  (void)f;
  assert(!(flags & 1));
}
static void wlr_ready(void *d, struct zwlr_screencopy_frame_v1 *f, uint32_t hi,
                      uint32_t lo, uint32_t ns) {
  (void)d;
  (void)f;
  (void)hi;
  (void)lo;
  (void)ns;
  done = true;
}
static void wlr_failed(void *d, struct zwlr_screencopy_frame_v1 *f) {
  (void)d;
  (void)f;
  failed = done = true;
}
static void wlr_damage(void *d, struct zwlr_screencopy_frame_v1 *f, uint32_t x,
                       uint32_t y, uint32_t w, uint32_t h) {
  (void)d;
  (void)f;
  (void)x;
  (void)y;
  (void)w;
  (void)h;
}
static void wlr_dma(void *d, struct zwlr_screencopy_frame_v1 *f, uint32_t fmt,
                    uint32_t w, uint32_t h) {
  (void)d;
  (void)f;
  if (use_dma) {
    format = fmt;
    width = w;
    height = h;
    have_format = true;
  }
}
static void wlr_buffer_done(void *d, struct zwlr_screencopy_frame_v1 *f) {
  (void)d;
  allocate();
  zwlr_screencopy_frame_v1_copy(f, buffer);
}
static const struct zwlr_screencopy_frame_v1_listener wlr_listener = {
    wlr_buffer, wlr_flags, wlr_ready,      wlr_failed,
    wlr_damage, wlr_dma,   wlr_buffer_done};
static void ext_size(void *d, struct ext_image_copy_capture_session_v1 *s,
                     uint32_t w, uint32_t h) {
  (void)d;
  (void)s;
  width = w;
  height = h;
}
static void ext_shm(void *d, struct ext_image_copy_capture_session_v1 *s,
                    uint32_t fmt) {
  (void)d;
  (void)s;
  if (!use_dma && fmt == WL_SHM_FORMAT_ARGB8888) {
    format = fmt;
    have_format = true;
  }
}
static void ext_device(void *d, struct ext_image_copy_capture_session_v1 *s,
                       struct wl_array *dev) {
  (void)d;
  (void)s;
  (void)dev;
}
static void ext_format(void *d, struct ext_image_copy_capture_session_v1 *s,
                       uint32_t fmt, struct wl_array *mods) {
  (void)d;
  (void)s;
  (void)mods;
  if (use_dma && fmt == GBM_FORMAT_ARGB8888) {
    format = fmt;
    have_format = true;
  }
}
static void ext_done(void *d, struct ext_image_copy_capture_session_v1 *s) {
  (void)d;
  (void)s;
  constraints_done = true;
}
static void ext_stopped(void *d, struct ext_image_copy_capture_session_v1 *s) {
  (void)d;
  (void)s;
  failed = done = true;
}
static const struct ext_image_copy_capture_session_v1_listener
    session_listener = {ext_size,   ext_shm,  ext_device,
                        ext_format, ext_done, ext_stopped};
static void ext_transform(void *d, struct ext_image_copy_capture_frame_v1 *f,
                          uint32_t t) {
  (void)d;
  (void)f;
  assert(t == WL_OUTPUT_TRANSFORM_NORMAL);
}
static void ext_damage(void *d, struct ext_image_copy_capture_frame_v1 *f,
                       int32_t x, int32_t y, int32_t w, int32_t h) {
  (void)d;
  (void)f;
  (void)x;
  (void)y;
  (void)w;
  (void)h;
}
static void ext_time(void *d, struct ext_image_copy_capture_frame_v1 *f,
                     uint32_t hi, uint32_t lo, uint32_t ns) {
  (void)d;
  (void)f;
  (void)hi;
  (void)lo;
  (void)ns;
}
static void ext_ready(void *d, struct ext_image_copy_capture_frame_v1 *f) {
  (void)d;
  (void)f;
  done = true;
}
static void ext_failed(void *d, struct ext_image_copy_capture_frame_v1 *f,
                       uint32_t reason) {
  (void)d;
  (void)f;
  fprintf(stderr, "capture failed: %u\n", reason);
  failed = done = true;
}
static const struct ext_image_copy_capture_frame_v1_listener frame_listener = {
    ext_transform, ext_damage, ext_time, ext_ready, ext_failed};
int main(int argc, char **argv) {
  if (argc != 6) {
    fprintf(stderr,
            "usage: %s wlr|ext|window OUTPUT_OR_APP shm|dma OUTPUT.ppm "
            "0|1(cursor)\n",
            argv[0]);
    return 2;
  }
  target_name = argv[2];
  use_dma = !strcmp(argv[3], "dma");
  alarm(15);
  display = wl_display_connect(NULL);
  assert(display);
  struct wl_registry *registry = wl_display_get_registry(display);
  wl_registry_add_listener(registry, &registry_listener, NULL);
  assert(wl_display_roundtrip(display) >= 0 &&
         wl_display_roundtrip(display) >= 0);
  struct wl_output *out = NULL;
  for (unsigned i = 0; i < output_count; i++)
    if (!strcmp(outputs[i].name, target_name))
      out = outputs[i].resource;
  if (!strcmp(argv[1], "wlr")) {
    assert(wlr && out);
    struct zwlr_screencopy_frame_v1 *frame =
        zwlr_screencopy_manager_v1_capture_output(wlr, atoi(argv[5]), out);
    zwlr_screencopy_frame_v1_add_listener(frame, &wlr_listener, NULL);
  } else {
    assert(ext);
    struct ext_image_capture_source_v1 *source;
    if (!strcmp(argv[1], "window")) {
      assert(window_sources && target_window);
      source =
          ext_foreign_toplevel_image_capture_source_manager_v1_create_source(
              window_sources, target_window);
    } else {
      assert(output_sources && out);
      source = ext_output_image_capture_source_manager_v1_create_source(
          output_sources, out);
    }
    struct ext_image_copy_capture_session_v1 *session =
        ext_image_copy_capture_manager_v1_create_session(ext, source,
                                                         atoi(argv[5]));
    ext_image_copy_capture_session_v1_add_listener(session, &session_listener,
                                                   NULL);
    while (!constraints_done && !done)
      assert(wl_display_dispatch(display) >= 0);
    assert(!failed);
    allocate();
    struct ext_image_copy_capture_frame_v1 *frame =
        ext_image_copy_capture_session_v1_create_frame(session);
    ext_image_copy_capture_frame_v1_add_listener(frame, &frame_listener, NULL);
    ext_image_copy_capture_frame_v1_attach_buffer(frame, buffer);
    ext_image_copy_capture_frame_v1_damage_buffer(frame, 0, 0, width, height);
    ext_image_copy_capture_frame_v1_capture(frame);
  }
  while (!done)
    assert(wl_display_dispatch(display) >= 0);
  if (failed)
    return 1;
  void *map_data = NULL;
  if (use_dma) {
    pixels = gbm_bo_map(bo, 0, 0, width, height, GBM_BO_TRANSFER_READ, &stride,
                        &map_data);
    assert(pixels);
  }
  FILE *file = fopen(argv[4], "wb");
  assert(file);
  fprintf(file, "P6\n%u %u\n255\n", width, height);
  for (uint32_t y = 0; y < height; y++)
    for (uint32_t x = 0; x < width; x++) {
      uint32_t p = *(uint32_t *)((uint8_t *)pixels + y * stride + x * 4);
      uint8_t rgb[] = {p >> 16, p >> 8, p};
      assert(fwrite(rgb, 1, 3, file) == 3);
    }
  fclose(file);
  if (use_dma)
    gbm_bo_unmap(bo, map_data);
  fprintf(stderr, "captured %ux%u via %s/%s\n", width, height, argv[1],
          argv[3]);
  wl_display_disconnect(display);
  return 0;
}
