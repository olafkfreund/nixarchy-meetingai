// Persistent virtual keyboard with a standard US keymap for compositor
// bindings.
#define _GNU_SOURCE
#include "virtual-keyboard-unstable-v1-client-protocol.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>
#include <wayland-client.h>
#include <xkbcommon/xkbcommon.h>

static struct wl_seat *seat;
static struct zwp_virtual_keyboard_manager_v1 *manager;
static void global(void *data, struct wl_registry *registry, uint32_t id,
                   const char *interface, uint32_t version) {
  (void)data;
  (void)version;
  if (!strcmp(interface, "wl_seat"))
    seat = wl_registry_bind(registry, id, &wl_seat_interface, 1);
  if (!strcmp(interface, "zwp_virtual_keyboard_manager_v1"))
    manager = wl_registry_bind(registry, id,
                               &zwp_virtual_keyboard_manager_v1_interface, 1);
}
static void removed(void *data, struct wl_registry *registry, uint32_t id) {
  (void)data;
  (void)registry;
  (void)id;
}
static const struct wl_registry_listener listener = {global, removed};

int main(void) {
  alarm(120);
  struct wl_display *display = wl_display_connect(NULL);
  assert(display);
  struct wl_registry *registry = wl_display_get_registry(display);
  wl_registry_add_listener(registry, &listener, NULL);
  assert(wl_display_roundtrip(display) >= 0 && seat && manager);
  struct xkb_context *context = xkb_context_new(XKB_CONTEXT_NO_FLAGS);
  const struct xkb_rule_names names = {.layout = "us"};
  struct xkb_keymap *keymap =
      xkb_keymap_new_from_names(context, &names, XKB_KEYMAP_COMPILE_NO_FLAGS);
  assert(keymap);
  char *text = xkb_keymap_get_as_string(keymap, XKB_KEYMAP_FORMAT_TEXT_V1);
  assert(text);
  size_t size = strlen(text) + 1;
  int fd = memfd_create("lennette-qa-keyboard", MFD_CLOEXEC);
  assert(fd >= 0 && ftruncate(fd, (off_t)size) == 0);
  assert(write(fd, text, size) == (ssize_t)size);
  struct zwp_virtual_keyboard_v1 *keyboard =
      zwp_virtual_keyboard_manager_v1_create_virtual_keyboard(manager, seat);
  zwp_virtual_keyboard_v1_keymap(keyboard, WL_KEYBOARD_KEYMAP_FORMAT_XKB_V1, fd,
                                 (uint32_t)size);
  assert(wl_display_roundtrip(display) >= 0);
  close(fd);
  free(text);
  uint32_t modifiers =
      (1u << xkb_keymap_mod_get_index(keymap, XKB_MOD_NAME_CTRL)) |
      (1u << xkb_keymap_mod_get_index(keymap, XKB_MOD_NAME_SHIFT));
  puts("ready");
  fflush(stdout);
  unsigned code;
  while (scanf("%u", &code) == 1) {
    struct timespec now;
    assert(clock_gettime(CLOCK_MONOTONIC, &now) == 0);
    uint32_t timestamp = (uint32_t)(now.tv_sec * 1000 + now.tv_nsec / 1000000);
    zwp_virtual_keyboard_v1_modifiers(keyboard, modifiers, 0, 0, 0);
    zwp_virtual_keyboard_v1_key(keyboard, timestamp, code,
                                WL_KEYBOARD_KEY_STATE_PRESSED);
    zwp_virtual_keyboard_v1_key(keyboard, timestamp + 1, code,
                                WL_KEYBOARD_KEY_STATE_RELEASED);
    zwp_virtual_keyboard_v1_modifiers(keyboard, 0, 0, 0, 0);
    assert(wl_display_roundtrip(display) >= 0);
    puts("sent");
    fflush(stdout);
  }
  xkb_keymap_unref(keymap);
  xkb_context_unref(context);
  wl_display_disconnect(display);
  return 0;
}
