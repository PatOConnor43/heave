POST {{ baseurl }}
Authorization: Bearer {{ authorization }}
```graphql
query {
  fallibleGetPet {
    ... on Pet {
      id
      name
      tag
      friend {
        id
        name
        tag
        friend {
          id
          name
          tag
          friend {
            id
            name
            tag
            friend {
              id
              name
              tag
            }
          }
        }
      }
    }
    ... on Error {
      code
      message
    }
  }
}
```
HTTP 200
