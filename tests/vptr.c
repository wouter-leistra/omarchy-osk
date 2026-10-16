/* test helper: drive the pointer from a script on stdin
 *   move X Y | down | up | sleep MS | path X1 Y1 X2 Y2 ... (drag through points) */
#include "proto/vp.h"
#include <linux/input-event-codes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
#include <wayland-client.h>

static struct zwlr_virtual_pointer_manager_v1 *mgr;
static struct wl_seat *seat;
static uint32_t W = 2160, H = 1440;

static void global(void *d, struct wl_registry *r, uint32_t name, const char *iface, uint32_t v) {
	if (!strcmp(iface, zwlr_virtual_pointer_manager_v1_interface.name))
		mgr = wl_registry_bind(r, name, &zwlr_virtual_pointer_manager_v1_interface, 1);
	else if (!strcmp(iface, wl_seat_interface.name))
		seat = wl_registry_bind(r, name, &wl_seat_interface, 1);
}
static void global_remove(void *d, struct wl_registry *r, uint32_t n) {}
static const struct wl_registry_listener rl = {global, global_remove};

static uint32_t now(void) { struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t); return t.tv_sec * 1000 + t.tv_nsec / 1000000; }

int main(void) {
	struct wl_display *dpy = wl_display_connect(NULL);
	struct wl_registry *reg = wl_display_get_registry(dpy);
	wl_registry_add_listener(reg, &rl, NULL);
	wl_display_roundtrip(dpy);
	if (!mgr) { fprintf(stderr, "no virtual pointer manager\n"); return 1; }
	struct zwlr_virtual_pointer_v1 *p = zwlr_virtual_pointer_manager_v1_create_virtual_pointer(mgr, seat);
	char cmd[64];
	double cx = 0, cy = 0;
	while (scanf("%63s", cmd) == 1) {
		if (!strcmp(cmd, "move") || !strcmp(cmd, "to")) {
			double x, y; scanf("%lf %lf", &x, &y);
			int steps = !strcmp(cmd, "to") ? 10 : 1;
			for (int i = 1; i <= steps; i++) {
				double px = cx + (x - cx) * i / steps, py = cy + (y - cy) * i / steps;
				zwlr_virtual_pointer_v1_motion_absolute(p, now(), px, py, W, H);
				zwlr_virtual_pointer_v1_frame(p);
				wl_display_flush(dpy);
				usleep(8000);
			}
			cx = x; cy = y;
		} else if (!strcmp(cmd, "down") || !strcmp(cmd, "up")) {
			zwlr_virtual_pointer_v1_button(p, now(), BTN_LEFT, !strcmp(cmd, "down"));
			zwlr_virtual_pointer_v1_frame(p);
		} else if (!strcmp(cmd, "sleep")) {
			int ms; scanf("%d", &ms);
			wl_display_flush(dpy);
			usleep(ms * 1000);
		}
		wl_display_roundtrip(dpy);
		usleep(15000);
	}
	zwlr_virtual_pointer_v1_destroy(p);
	wl_display_roundtrip(dpy);
	return 0;
}
