POST {{ baseurl }}/pet/{{petId}}/uploadImage
Authorization: Bearer {{ authorization }}
Prefer: code=200

[QueryStringParams]
additionalMetadata:


HTTP 200

[Asserts]
jsonpath "$" isCollection
#jsonpath "$.code" isInteger
#jsonpath "$.type" isString
#jsonpath "$.message" isString
