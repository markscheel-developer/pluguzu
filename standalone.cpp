// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0-or-later

// A standalone program to validate the pluguzu wrapper.
#include <cstdint>
#include <cstdio>

#include "Pluguzu_stub.h"

struct me {
  double time;
  int8_t pitch;
  int8_t vel;
};

extern "C" void pluguzu_init(void);

int main() {
  pluguzu_init();
  auto state = pluguzuNew();
  pluguzuParse(state, (HsPtr) "n \"[c3 f3]\"");
  const char *err = (char *)pluguzuError(state);
  if (err != NULL) {
    printf("Parse error: %s\n", err);
    return 1;
  }
  me evs[512];
  auto count = (uint32_t)pluguzuRender(state, 4, evs);
  printf("Got %d\n", count);
  for (uint32_t i = 0; i < count; i++) {
    me *ev = &evs[i];
    printf("=> %2.4f %d %d\n", ev->time, ev->pitch, ev->vel);
  }
  return 0;
}
