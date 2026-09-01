// SPDX-License-Identifier: Apache-2.0
#ifndef BRIDGEVM_CATALOG_CONTENT_DIGEST_H
#define BRIDGEVM_CATALOG_CONTENT_DIGEST_H

#include <openssl/pkcs7.h>

int bridgevm_catalog_content_digest_valid(PKCS7 *message);

#endif
