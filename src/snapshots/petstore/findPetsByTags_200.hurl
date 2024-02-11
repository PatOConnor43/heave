GET {{ baseurl }}/pet/findByTags
Authorization: Bearer {{ authorization }}
Prefer: code=200

[QueryStringParams]
tags:

HTTP 200

[Asserts]
jsonpath "$" isCollection
#jsonpath "$[0]" isCollection
#jsonpath "$[0].id" isInteger
#jsonpath "$[0].name" isString
#jsonpath "$[0].category" isCollection
#jsonpath "$[0].category.id" isInteger
#jsonpath "$[0].category.name" isString
#jsonpath "$[0].photoUrls" isCollection
#jsonpath "$[0].photoUrls[0]" isString
#jsonpath "$[0].tags" isCollection
#jsonpath "$[0].tags[0]" isCollection
#jsonpath "$[0].tags[0].id" isInteger
#jsonpath "$[0].tags[0].name" isString
#jsonpath "$[0].status" isString
