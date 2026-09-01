// SPDX-License-Identifier: Apache-2.0
#include "bridgevm-catalog-content-digest.h"

#include <openssl/asn1.h>
#include <openssl/crypto.h>
#include <openssl/evp.h>
#include <openssl/objects.h>
#include <openssl/x509.h>

static ASN1_TYPE *unique_attribute(PKCS7_SIGNER_INFO *signer, int nid) {
    const int index = X509at_get_attr_by_NID(signer->auth_attr, nid, -1);
    if (index < 0 || X509at_get_attr_by_NID(signer->auth_attr, nid, index) >= 0) {
        return NULL;
    }
    X509_ATTRIBUTE *attribute = X509at_get_attr(signer->auth_attr, index);
    if (attribute == NULL || X509_ATTRIBUTE_count(attribute) != 1) {
        return NULL;
    }
    return X509_ATTRIBUTE_get0_type(attribute, 0);
}

static int is_windows_catalog_content(PKCS7 *content) {
    ASN1_OBJECT *catalog_oid = OBJ_txt2obj("1.3.6.1.4.1.311.10.1", 1);
    const int matches = catalog_oid != NULL && content != NULL &&
        OBJ_cmp(content->type, catalog_oid) == 0;
    ASN1_OBJECT_free(catalog_oid);
    return matches;
}
// PKCS#7 hashes content value octets, excluding the content DER tag and length.
static int encoded_content_value(
    PKCS7 *content, unsigned char **allocation,
    const unsigned char **value, long *value_length
) {
    if (content->d.other == NULL || (content->d.other->type != V_ASN1_SEQUENCE &&
        content->d.other->type != V_ASN1_OCTET_STRING)) {
        return 0;
    }
    const int encoded_length = i2d_ASN1_TYPE(content->d.other, allocation);
    if (encoded_length <= 0) {
        return 0;
    }
    *value = *allocation;
    int tag = 0;
    int object_class = 0;
    const int flags = ASN1_get_object(
        value, value_length, &tag, &object_class, encoded_length);
    return (flags & 0x80) == 0 && tag == content->d.other->type &&
        object_class == V_ASN1_UNIVERSAL && *value_length >= 0 &&
        *value + *value_length == *allocation + encoded_length;
}

static int signer_digest_matches(
    PKCS7_SIGNER_INFO *signer, ASN1_OBJECT *content_type,
    const unsigned char *content, size_t content_length
) {
    ASN1_TYPE *type = unique_attribute(signer, NID_pkcs9_contentType);
    ASN1_TYPE *expected = unique_attribute(signer, NID_pkcs9_messageDigest);
    if (type == NULL || type->type != V_ASN1_OBJECT ||
        OBJ_cmp(type->value.object, content_type) != 0 || expected == NULL ||
        expected->type != V_ASN1_OCTET_STRING ||
        OBJ_obj2nid(signer->digest_alg->algorithm) != NID_sha256) {
        return 0;
    }
    unsigned char actual[EVP_MAX_MD_SIZE];
    unsigned int actual_length = 0;
    if (EVP_Digest(content, content_length, actual, &actual_length,
                   EVP_sha256(), NULL) != 1) {
        return 0;
    }
    ASN1_OCTET_STRING *declared = expected->value.octet_string;
    return ASN1_STRING_length(declared) == (int)actual_length &&
        CRYPTO_memcmp(ASN1_STRING_get0_data(declared), actual, actual_length) == 0;
}

int bridgevm_catalog_content_digest_valid(PKCS7 *message) {
    if (message == NULL || message->d.sign == NULL ||
        !is_windows_catalog_content(message->d.sign->contents)) {
        return 0;
    }
    PKCS7 *content_info = message->d.sign->contents;
    unsigned char *allocation = NULL;
    const unsigned char *content = NULL;
    long content_length = 0;
    if (!encoded_content_value(
            content_info, &allocation, &content, &content_length)) {
        OPENSSL_free(allocation);
        return 0;
    }
    STACK_OF(PKCS7_SIGNER_INFO) *signers = message->d.sign->signer_info;
    int valid = signers != NULL && sk_PKCS7_SIGNER_INFO_num(signers) > 0;
    for (int index = 0; valid && index < sk_PKCS7_SIGNER_INFO_num(signers); index++) {
        valid = signer_digest_matches(
            sk_PKCS7_SIGNER_INFO_value(signers, index), content_info->type,
            content, (size_t)content_length);
    }
    OPENSSL_free(allocation);
    return valid;
}
