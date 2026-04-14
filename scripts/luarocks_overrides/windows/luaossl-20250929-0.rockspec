-- Windows override / Windows 覆盖版
-- 说明 / Notes:
-- 将 luaossl 在 Windows 下仍然沿用的 OpenSSL 1.x 库名切换为项目预编译产物中的 OpenSSL 3 库名，
-- 同时补充 crypt32/user32 依赖，避免外部依赖探测和最终链接阶段失败。
-- Switch luaossl's legacy OpenSSL 1.x Windows library names to the OpenSSL 3 names used by the project's prebuilt artifacts,
-- and add crypt32/user32 so both dependency discovery and final linking succeed.

package = "luaossl"
version = "20250929-0"

source = {
  url = "https://github.com/wahern/luaossl/archive/rel-20250929.zip";
  md5 = "ddc1b75de8d8084eef15d3a31a241155";
  dir = "luaossl-rel-20250929";
}

description = {
  summary = "Most comprehensive OpenSSL module in the Lua universe.";
  homepage = "http://25thandclement.com/~william/projects/luaossl.html";
  license = "MIT/X11";
}

supported_platforms = {
  "unix";
  "windows";
}

dependencies = {
  "lua";
}

external_dependencies = {
  OPENSSL = {
    header = "openssl/ssl.h";
    library = "ssl";
  };
  CRYPTO = {
    header = "openssl/crypto.h";
    library = "crypto";
  };
  platforms = {
    windows = {
      OPENSSL = {
        library = "libssl"
      };
      CRYPTO = {
        library = "libcrypto"
      };
    };
  };
}

build = {
  type = "builtin";
  modules = {
    ["_openssl"] = {
      sources = {
        "src/openssl.c";
        "vendor/compat53/c-api/compat-5.3.c";
      };
      libraries = {
        "ssl";
        "crypto";
      };
      defines = {
        "_REENTRANT"; "_THREAD_SAFE";
        "COMPAT53_PREFIX=luaossl";
      };
      incdirs = {
        "$(OPENSSL_INCDIR)";
        "$(CRYPTO_INCDIR)";
      };
      libdirs = {
        "$(OPENSSL_LIBDIR)";
        "$(CRYPTO_LIBDIR)";
      };
    };
    ["openssl"] = "src/openssl.lua";
    ["openssl.auxlib"] = "src/openssl.auxlib.lua";
    ["openssl.bignum"] = "src/openssl.bignum.lua";
    ["openssl.cipher"] = "src/openssl.cipher.lua";
    ["openssl.des"] = "src/openssl.des.lua";
    ["openssl.digest"] = "src/openssl.digest.lua";
    ["openssl.hmac"] = "src/openssl.hmac.lua";
    ["openssl.kdf"] = "src/openssl.kdf.lua";
    ["openssl.ocsp.basic"] = "src/openssl.ocsp.basic.lua";
    ["openssl.ocsp.response"] = "src/openssl.ocsp.response.lua";
    ["openssl.pkcs12"] = "src/openssl.pkcs12.lua";
    ["openssl.pkey"] = "src/openssl.pkey.lua";
    ["openssl.pubkey"] = "src/openssl.pubkey.lua";
    ["openssl.rand"] = "src/openssl.rand.lua";
    ["openssl.ssl.context"] = "src/openssl.ssl.context.lua";
    ["openssl.ssl"] = "src/openssl.ssl.lua";
    ["openssl.x509"] = "src/openssl.x509.lua";
    ["openssl.x509.altname"] = "src/openssl.x509.altname.lua";
    ["openssl.x509.chain"] = "src/openssl.x509.chain.lua";
    ["openssl.x509.crl"] = "src/openssl.x509.crl.lua";
    ["openssl.x509.csr"] = "src/openssl.x509.csr.lua";
    ["openssl.x509.extension"] = "src/openssl.x509.extension.lua";
    ["openssl.x509.name"] = "src/openssl.x509.name.lua";
    ["openssl.x509.store"] = "src/openssl.x509.store.lua";
    ["openssl.x509.verify_param"] = "src/openssl.x509.verify_param.lua";
  };
  platforms = {
    unix = {
      modules = {
        ["_openssl"] = {
          libraries = {
            nil, nil;
            "pthread";
            "m";
          };
          defines = {
            nil, nil, nil;
            "_GNU_SOURCE";
          }
        };
      };
    };
    linux = {
      modules = {
        ["_openssl"] = {
          libraries = {
            nil, nil, nil, nil;
            "dl";
          };
        };
      };
    };
    win32 = {
      modules = {
        ["_openssl"] = {
          libraries = {
            "libssl";
            "libcrypto";
            "ws2_32";
            "advapi32";
            "crypt32";
            "user32";
            "kernel32";
          };
          defines = {
            nil, nil, nil;
            "HAVE_SYS_PARAM_H=0";
            "HAVE_DLFCN_H=0";
            "_WIN32_WINNT=0x0600";
          };
        };
      };
    };
  };
  patches = {
    ["config.h.diff"] = [[
--- a/src/openssl.c
+++ b/src/openssl.c
@@ -26,3 +26 @@
-#if HAVE_CONFIG_H
-#include "config.h"
-#endif
+#include "../config.h.guess"
]];
  }
}
