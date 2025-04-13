POST {{ baseurl }}
Authorization: Bearer {{ authorization }}
```graphql
query {
  getPet(id: {{ getPet_id }}) {
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
}
```
HTTP 200
