#include <stdlib.h>
#include <stdio.h>
#include "HsFFI.h"

HsBool pluguzu_init(void){
  int argc = 2;
  char *argv[] = { "+RTS", "-A32m", NULL };
  char **pargv = argv;

  // Initialize Haskell runtime
  hs_init(&argc, &pargv);
  printf("HS INIT!\n");

  // do any other initialization here and
  // return false if there was a problem
  return HS_BOOL_TRUE;
}

void pluguzu_exit(void){
  hs_exit();
  printf("HS EXIT!\n");
}
