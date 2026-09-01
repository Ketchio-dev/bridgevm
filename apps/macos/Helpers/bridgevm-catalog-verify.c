// SPDX-License-Identifier: Apache-2.0
#include "bridgevm-catalog-content-digest.h"

#include <errno.h>
#include <openssl/crypto.h>
#include <openssl/err.h>
#include <openssl/objects.h>
#include <openssl/pkcs7.h>
#include <openssl/x509.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>

enum { MAX_CATALOG_BYTES = 32 * 1024 * 1024 };

static int fail(const char *code) {
    fprintf(stderr, "bridgevm-catalog-verify: %s\n", code);
    return 1;
}

int main(int argc, char **argv) {
    if (argc == 2 && strcmp(argv[1], "--version") == 0) {
        printf("bridgevm-catalog-verify-v1 libcrypto=%s\n", OpenSSL_version(OPENSSL_VERSION));
        return 0;
    }
    if (argc != 2) {
        fprintf(stderr, "usage: bridgevm-catalog-verify CAT\n");
        return 2;
    }

    struct stat metadata;
    if (lstat(argv[1], &metadata) != 0 || !S_ISREG(metadata.st_mode) ||
        metadata.st_size <= 0 || metadata.st_size > MAX_CATALOG_BYTES) {
        return fail("invalid-input");
    }

    BIO *input = BIO_new_file(argv[1], "rb");
    if (input == NULL) {
        return fail("read-failed");
    }
    PKCS7 *message = d2i_PKCS7_bio(input, NULL);
    BIO_free(input);
    if (message == NULL || !PKCS7_type_is_signed(message) ||
        PKCS7_get_detached(message) != 0) {
        PKCS7_free(message);
        ERR_clear_error();
        return fail("invalid-signed-data");
    }

    STACK_OF(X509) *signers = PKCS7_get0_signers(message, NULL, PKCS7_NOVERIFY);
    if (signers == NULL || sk_X509_num(signers) < 1) {
        sk_X509_free(signers);
        PKCS7_free(message);
        ERR_clear_error();
        return fail("missing-signer");
    }
    sk_X509_free(signers);

    BIO *sink = BIO_new(BIO_s_null());
    if (sink == NULL) {
        PKCS7_free(message);
        return fail("internal-error");
    }
    const int verified = PKCS7_verify(
        message, NULL, NULL, NULL, sink, PKCS7_NOVERIFY | PKCS7_BINARY);
    const int digest_verified = bridgevm_catalog_content_digest_valid(message);
    BIO_free(sink);
    PKCS7_free(message);
    if (verified != 1) {
        ERR_clear_error();
        return fail("signature-invalid");
    }
    if (digest_verified != 1) {
        return fail("content-digest-invalid");
    }
    return 0;
}
