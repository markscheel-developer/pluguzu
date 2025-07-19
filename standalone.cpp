// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

// A standalone program to validate the pluguzu wrapper.
#include <cstdint>
#include <cstdio>

#include "Pluguzu_stub.h"

struct me {
  double time;
  uint8_t d0;
  uint8_t d1;
  uint8_t d2;
};

extern "C" void pluguzu_init(void);

int main(int argc, const char **argv) {
  pluguzu_init();
  auto state = pluguzuNew(1024);
  const char *code = argc > 1 ? argv[1] : "n \"[c3 f3]\"";
  pluguzuSoundMap(state, (HsPtr) "bd", 42);
  pluguzuSoundMap(state, (HsPtr) "sd", 23);
  pluguzuParse(state, (HsPtr)code);
  const char *err = (char *)pluguzuError(state);
  if (err != NULL) {
    printf("Parse error: %s\n", err);
    return 1;
  }
  me evs[1024];
  auto count = (uint32_t)pluguzuRender(state, 4, 8, evs);
  printf("Got %d\n", count);
  for (uint32_t i = 0; i < count; i++) {
    me *ev = &evs[i];
    printf("=> %2.4f %X %d %d\n", ev->time, ev->d0, ev->d1, ev->d2);
  }
  return 0;
}
